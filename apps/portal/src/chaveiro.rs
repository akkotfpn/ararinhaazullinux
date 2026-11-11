// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-chaveiro: o chaveiro de senhas do Ararinha (Secret Service,
//! org.freedesktop.secrets). Chromium, Discord, Spotify, VS Code e outros
//! guardam logins e tokens aqui em vez de num arquivo solto.
//!
//! Uma coleção só, "login" (o alias "default"), sempre destravada durante
//! a sessão. Os segredos ficam em ~/.local/share/ararinha/chaveiro
//! (0600, pasta 0700); com o disco criptografado (padrão do instalador),
//! eles ficam cifrados no disco junto com o resto.
//! Sessões só no modo "plain": o segredo nunca sai do computador; ele
//! passa pelo barramento da própria sessão, que só o usuário acessa.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use zbus::{
    fdo, interface,
    object_server::{ObjectServer, SignalEmitter},
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

const RAIZ: &str = "/org/freedesktop/secrets";
const COLECAO: &str = "/org/freedesktop/secrets/collection/login";
const ALIAS: &str = "/org/freedesktop/secrets/aliases/default";

type Segredo = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

#[derive(Clone, Default)]
struct Dado {
    id: u64,
    rotulo: String,
    atributos: HashMap<String, String>,
    segredo: Vec<u8>,
    tipo: String,
    criado: u64,
    mudado: u64,
}

#[derive(Default)]
struct Banco {
    itens: Vec<Dado>,
    proximo: u64,
    sessoes: u64,
}

type Estado = Arc<Mutex<Banco>>;

fn agora() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn arquivo() -> PathBuf {
    let casa = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(casa).join(".local/share/ararinha/chaveiro")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn de_hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()).collect()
}

fn atributos_texto(a: &HashMap<String, String>) -> String {
    let mut v: Vec<String> = a.iter().map(|(k, v)| format!("{}={}", hex(k.as_bytes()), hex(v.as_bytes()))).collect();
    v.sort();
    v.join(",")
}

fn carregar() -> Banco {
    let mut b = Banco::default();
    let texto = std::fs::read_to_string(arquivo()).unwrap_or_default();
    for l in texto.lines() {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() < 7 {
            continue;
        }
        let atributos = c[2]
            .split(',')
            .filter(|p| !p.is_empty())
            .filter_map(|p| {
                let (k, v) = p.split_once('=')?;
                Some((String::from_utf8_lossy(&de_hex(k)).to_string(), String::from_utf8_lossy(&de_hex(v)).to_string()))
            })
            .collect();
        let id: u64 = c[0].parse().unwrap_or(0);
        b.proximo = b.proximo.max(id + 1);
        b.itens.push(Dado {
            id,
            rotulo: String::from_utf8_lossy(&de_hex(c[1])).to_string(),
            atributos,
            segredo: de_hex(c[3]),
            tipo: String::from_utf8_lossy(&de_hex(c[4])).to_string(),
            criado: c[5].parse().unwrap_or(0),
            mudado: c[6].parse().unwrap_or(0),
        });
    }
    b
}

fn salvar(b: &Banco) {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let arq = arquivo();
    if let Some(d) = arq.parent() {
        let _ = std::fs::create_dir_all(d);
        let _ = std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700));
    }
    let mut texto = String::new();
    for d in &b.itens {
        texto += &format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            d.id,
            hex(d.rotulo.as_bytes()),
            atributos_texto(&d.atributos),
            hex(&d.segredo),
            hex(d.tipo.as_bytes()),
            d.criado,
            d.mudado
        );
    }
    let tmp = arq.with_extension("novo");
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp) {
        if f.write_all(texto.as_bytes()).is_ok() && f.sync_all().is_ok() {
            let _ = std::fs::rename(&tmp, &arq);
        }
    }
}

fn caminho_item(id: u64) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{COLECAO}/{id}")).expect("caminho válido")
}

fn sem_prompt() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").expect("caminho válido")
}

fn combina(d: &Dado, filtro: &HashMap<String, String>) -> bool {
    filtro.iter().all(|(k, v)| d.atributos.get(k) == Some(v))
}

fn segredo_de(d: &Dado, sessao: &OwnedObjectPath) -> Segredo {
    (sessao.clone(), Vec::new(), d.segredo.clone(), if d.tipo.is_empty() { "text/plain".into() } else { d.tipo.clone() })
}

fn texto_de(v: &OwnedValue) -> Option<String> {
    match &**v {
        Value::Str(s) => Some(s.to_string()),
        Value::Value(i) => match &**i {
            Value::Str(s) => Some(s.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn mapa_de(v: &OwnedValue) -> HashMap<String, String> {
    let copia = match v.try_clone() {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let interno: OwnedValue = match &*copia {
        Value::Value(i) => match OwnedValue::try_from((**i).try_clone().unwrap_or(Value::from(0u8))) {
            Ok(o) => o,
            Err(_) => return HashMap::new(),
        },
        _ => copia,
    };
    HashMap::<String, String>::try_from(interno).unwrap_or_default()
}

// ---------------------------------------------------------------- objetos

struct Sessao;

#[interface(name = "org.freedesktop.Secret.Session")]
impl Sessao {
    async fn close(&self, #[zbus(object_server)] servidor: &ObjectServer, #[zbus(header)] cab: zbus::message::Header<'_>) {
        if let Some(p) = cab.path() {
            let p = p.to_owned();
            let _ = servidor.remove::<Sessao, _>(p).await;
        }
    }
}

struct Item {
    id: u64,
    estado: Estado,
}

#[interface(name = "org.freedesktop.Secret.Item")]
impl Item {
    async fn delete(&self, #[zbus(object_server)] servidor: &ObjectServer) -> OwnedObjectPath {
        {
            let mut b = self.estado.lock().unwrap();
            b.itens.retain(|d| d.id != self.id);
            salvar(&b);
        }
        let _ = servidor.remove::<Item, _>(caminho_item(self.id)).await;
        sem_prompt()
    }

    fn get_secret(&self, sessao: OwnedObjectPath) -> fdo::Result<Segredo> {
        let b = self.estado.lock().unwrap();
        b.itens
            .iter()
            .find(|d| d.id == self.id)
            .map(|d| segredo_de(d, &sessao))
            .ok_or_else(|| fdo::Error::UnknownObject("item apagado".into()))
    }

    fn set_secret(&self, segredo: Segredo) {
        let mut b = self.estado.lock().unwrap();
        if let Some(d) = b.itens.iter_mut().find(|d| d.id == self.id) {
            d.segredo = segredo.2;
            d.tipo = segredo.3;
            d.mudado = agora();
        }
        salvar(&b);
    }

    #[zbus(property)]
    fn locked(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn attributes(&self) -> HashMap<String, String> {
        let b = self.estado.lock().unwrap();
        b.itens.iter().find(|d| d.id == self.id).map(|d| d.atributos.clone()).unwrap_or_default()
    }

    #[zbus(property)]
    fn set_attributes(&mut self, a: HashMap<String, String>) {
        let mut b = self.estado.lock().unwrap();
        if let Some(d) = b.itens.iter_mut().find(|d| d.id == self.id) {
            d.atributos = a;
            d.mudado = agora();
        }
        salvar(&b);
    }

    #[zbus(property)]
    fn label(&self) -> String {
        let b = self.estado.lock().unwrap();
        b.itens.iter().find(|d| d.id == self.id).map(|d| d.rotulo.clone()).unwrap_or_default()
    }

    #[zbus(property)]
    fn set_label(&mut self, r: String) {
        let mut b = self.estado.lock().unwrap();
        if let Some(d) = b.itens.iter_mut().find(|d| d.id == self.id) {
            d.rotulo = r;
            d.mudado = agora();
        }
        salvar(&b);
    }

    #[zbus(property)]
    fn created(&self) -> u64 {
        let b = self.estado.lock().unwrap();
        b.itens.iter().find(|d| d.id == self.id).map(|d| d.criado).unwrap_or(0)
    }

    #[zbus(property)]
    fn modified(&self) -> u64 {
        let b = self.estado.lock().unwrap();
        b.itens.iter().find(|d| d.id == self.id).map(|d| d.mudado).unwrap_or(0)
    }
}

struct Colecao {
    estado: Estado,
}

#[interface(name = "org.freedesktop.Secret.Collection")]
impl Colecao {
    fn delete(&self) -> fdo::Result<OwnedObjectPath> {
        Err(fdo::Error::AccessDenied("o chaveiro de login não pode ser apagado".into()))
    }

    fn search_items(&self, atributos: HashMap<String, String>) -> Vec<OwnedObjectPath> {
        let b = self.estado.lock().unwrap();
        b.itens.iter().filter(|d| combina(d, &atributos)).map(|d| caminho_item(d.id)).collect()
    }

    async fn create_item(
        &self,
        propriedades: HashMap<String, OwnedValue>,
        segredo: Segredo,
        substituir: bool,
        #[zbus(object_server)] servidor: &ObjectServer,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        let rotulo = propriedades.get("org.freedesktop.Secret.Item.Label").and_then(texto_de).unwrap_or_default();
        let atributos = propriedades.get("org.freedesktop.Secret.Item.Attributes").map(mapa_de).unwrap_or_default();
        let (id, novo) = {
            let mut b = self.estado.lock().unwrap();
            let existente = if substituir && !atributos.is_empty() {
                b.itens.iter().position(|d| d.atributos == atributos)
            } else {
                None
            };
            let r = if let Some(i) = existente {
                let d = &mut b.itens[i];
                d.rotulo = rotulo;
                d.segredo = segredo.2;
                d.tipo = segredo.3;
                d.mudado = agora();
                (d.id, false)
            } else {
                let id = b.proximo;
                b.proximo += 1;
                let t = agora();
                b.itens.push(Dado { id, rotulo, atributos, segredo: segredo.2, tipo: segredo.3, criado: t, mudado: t });
                (id, true)
            };
            salvar(&b);
            r
        };
        if novo {
            let _ = servidor.at(caminho_item(id), Item { id, estado: self.estado.clone() }).await;
            let _ = Colecao::item_created(&em, caminho_item(id).as_ref()).await;
        }
        (caminho_item(id), sem_prompt())
    }

    #[zbus(signal)]
    async fn item_created(em: &SignalEmitter<'_>, item: ObjectPath<'_>) -> zbus::Result<()>;

    #[zbus(property)]
    fn items(&self) -> Vec<OwnedObjectPath> {
        let b = self.estado.lock().unwrap();
        b.itens.iter().map(|d| caminho_item(d.id)).collect()
    }

    #[zbus(property)]
    fn label(&self) -> String {
        "Login".into()
    }

    #[zbus(property)]
    fn locked(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn created(&self) -> u64 {
        0
    }

    #[zbus(property)]
    fn modified(&self) -> u64 {
        0
    }
}

struct Servico {
    estado: Estado,
}

#[interface(name = "org.freedesktop.Secret.Service")]
impl Servico {
    async fn open_session(
        &self,
        algoritmo: &str,
        _entrada: OwnedValue,
        #[zbus(object_server)] servidor: &ObjectServer,
    ) -> fdo::Result<(OwnedValue, OwnedObjectPath)> {
        if algoritmo != "plain" {
            // O libsecret tenta o "dh-ietf..." e volta para o "plain".
            return Err(fdo::Error::NotSupported("só o algoritmo plain".into()));
        }
        let n = {
            let mut b = self.estado.lock().unwrap();
            b.sessoes += 1;
            b.sessoes
        };
        let caminho = OwnedObjectPath::try_from(format!("{RAIZ}/session/{n}")).map_err(|e| fdo::Error::Failed(e.to_string()))?;
        let _ = servidor.at(caminho.clone(), Sessao).await;
        Ok((OwnedValue::try_from(Value::from("")).map_err(|e| fdo::Error::Failed(e.to_string()))?, caminho))
    }

    fn create_collection(&self, _props: HashMap<String, OwnedValue>, _alias: &str) -> (OwnedObjectPath, OwnedObjectPath) {
        (OwnedObjectPath::try_from(COLECAO).expect("caminho"), sem_prompt())
    }

    fn search_items(&self, atributos: HashMap<String, String>) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        let b = self.estado.lock().unwrap();
        (b.itens.iter().filter(|d| combina(d, &atributos)).map(|d| caminho_item(d.id)).collect(), Vec::new())
    }

    fn unlock(&self, objetos: Vec<OwnedObjectPath>) -> (Vec<OwnedObjectPath>, OwnedObjectPath) {
        (objetos, sem_prompt())
    }

    fn lock(&self, _objetos: Vec<OwnedObjectPath>) -> (Vec<OwnedObjectPath>, OwnedObjectPath) {
        (Vec::new(), sem_prompt())
    }

    fn get_secrets(&self, itens: Vec<OwnedObjectPath>, sessao: OwnedObjectPath) -> HashMap<OwnedObjectPath, Segredo> {
        let b = self.estado.lock().unwrap();
        itens
            .into_iter()
            .filter_map(|p| {
                let id: u64 = p.as_str().rsplit('/').next()?.parse().ok()?;
                let d = b.itens.iter().find(|d| d.id == id)?;
                Some((p.clone(), segredo_de(d, &sessao)))
            })
            .collect()
    }

    fn read_alias(&self, nome: &str) -> OwnedObjectPath {
        if nome == "default" || nome == "login" {
            OwnedObjectPath::try_from(COLECAO).expect("caminho")
        } else {
            sem_prompt()
        }
    }

    fn set_alias(&self, _nome: &str, _colecao: OwnedObjectPath) {}

    #[zbus(property)]
    fn collections(&self) -> Vec<OwnedObjectPath> {
        vec![OwnedObjectPath::try_from(COLECAO).expect("caminho")]
    }
}

fn main() -> zbus::Result<()> {
    let estado: Estado = Arc::new(Mutex::new(carregar()));
    let ids: Vec<u64> = estado.lock().unwrap().itens.iter().map(|d| d.id).collect();
    let mut b = zbus::blocking::connection::Builder::session()?
        .serve_at(RAIZ, Servico { estado: estado.clone() })?
        .serve_at(COLECAO, Colecao { estado: estado.clone() })?
        .serve_at(ALIAS, Colecao { estado: estado.clone() })?;
    for id in ids {
        b = b.serve_at(caminho_item(id), Item { id, estado: estado.clone() })?;
    }
    let _con = b.name("org.freedesktop.secrets")?.build()?;
    loop {
        std::thread::park();
    }
}
