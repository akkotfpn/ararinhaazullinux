// SPDX-License-Identifier: GPL-3.0-or-later
//! ararinha-bandeja: a bandeja do sistema (perto do relógio), como no
//! Windows. Implementa o StatusNotifierWatcher (o protocolo que a Steam,
//! o Discord, o Telegram e outros usam para pôr o ícone na bandeja) e lê
//! cada ícone registrado.
//!
//! Conversa com a Plumagem por arquivos e FIFOs em
//! $XDG_RUNTIME_DIR/ararinha-bandeja/:
//! - `itens`: uma linha por ícone: "n<TAB>título<TAB>arquivo do ícone";
//! - `n.argb`: ícone em pixels (u32 largura, u32 altura, ARGB, big-endian);
//! - `menu-n`: menu do ícone: "id<TAB>rótulo<TAB>flags" (s=separador,
//!   d=desativado, m=marcado);
//! - `ctl`: pedidos da Plumagem: "ativar n x y", "secundario n x y",
//!   "menu n", "clicar n id".
//! Quando algo muda, avisa a Plumagem pelo FIFO dela (plumagem-ctl).

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use zbus::{
    blocking::{fdo::DBusProxy, Connection, Proxy},
    interface,
    message::Header,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

#[derive(Clone, PartialEq, Debug)]
struct Item {
    /// Nome no barramento e caminho do objeto.
    servico: String,
    caminho: String,
}

type Itens = Arc<Mutex<Vec<Item>>>;

struct Vigia {
    itens: Itens,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Vigia {
    async fn register_status_notifier_item(
        &self,
        servico: &str,
        #[zbus(header)] cab: Header<'_>,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>,
    ) {
        let remetente = cab.sender().map(|s| s.to_string()).unwrap_or_default();
        // O app pode mandar o nome no barramento ou só o caminho do objeto.
        let item = if servico.starts_with('/') {
            Item { servico: remetente, caminho: servico.to_string() }
        } else {
            Item { servico: servico.to_string(), caminho: "/StatusNotifierItem".to_string() }
        };
        let nome = format!("{}{}", item.servico, item.caminho);
        {
            let mut v = self.itens.lock().unwrap();
            if !v.contains(&item) {
                v.push(item);
            }
        }
        let _ = Vigia::status_notifier_item_registered(&em, &nome).await;
    }

    async fn register_status_notifier_host(&self, _servico: &str) {}

    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.itens.lock().unwrap().iter().map(|i| format!("{}{}", i.servico, i.caminho)).collect()
    }

    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(em: &SignalEmitter<'_>, servico: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(em: &SignalEmitter<'_>, servico: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(em: &SignalEmitter<'_>) -> zbus::Result<()>;
}

fn pasta() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(base).join("ararinha-bandeja")
}

/// Avisa a Plumagem (sem travar se ela não estiver lendo).
fn avisar_plumagem(pedido: &str) {
    let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    let ctl = format!("{base}/plumagem-ctl");
    // O open de um FIFO sem leitor travaria: O_NONBLOCK devolve erro.
    use std::os::unix::fs::OpenOptionsExt;
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).custom_flags(0o4000).open(&ctl) {
        let _ = f.write_all(format!("{pedido}\n").as_bytes());
    }
}

fn proxy<'a>(con: &'a Connection, item: &'a Item) -> Option<Proxy<'a>> {
    Proxy::new(con, item.servico.as_str(), item.caminho.as_str(), "org.kde.StatusNotifierItem").ok()
}

/// Ícone do item: os pixels (IconPixmap, o maior até 64 px) ou um PNG pelo
/// nome (IconName), procurado no IconThemePath e nos temas de ícones.
fn icone(p: &Proxy, n: usize) -> Option<PathBuf> {
    if let Ok(pix) = p.get_property::<Vec<(i32, i32, Vec<u8>)>>("IconPixmap") {
        let melhor = pix
            .iter()
            .filter(|(l, a, d)| *l > 0 && *a > 0 && d.len() == (*l as usize) * (*a as usize) * 4)
            .min_by_key(|(l, _, _)| (*l - 32).abs());
        if let Some((l, a, d)) = melhor {
            let mut bytes = Vec::with_capacity(8 + d.len());
            bytes.extend_from_slice(&(*l as u32).to_le_bytes());
            bytes.extend_from_slice(&(*a as u32).to_le_bytes());
            bytes.extend_from_slice(d);
            let arq = pasta().join(format!("{n}.argb"));
            let tmp = arq.with_extension("tmp");
            if std::fs::write(&tmp, &bytes).is_ok() && std::fs::rename(&tmp, &arq).is_ok() {
                return Some(arq);
            }
        }
    }
    let nome: String = p.get_property("IconName").ok()?;
    if nome.is_empty() {
        return None;
    }
    if nome.starts_with('/') && Path::new(&nome).exists() {
        return Some(PathBuf::from(nome));
    }
    let tema: String = p.get_property("IconThemePath").unwrap_or_default();
    let casa = std::env::var("HOME").unwrap_or_default();
    let mut bases = Vec::new();
    if !tema.is_empty() {
        bases.push(tema.clone());
    }
    bases.push(format!("{casa}/.local/share/flatpak/exports/share/icons"));
    bases.push("/var/lib/flatpak/exports/share/icons".into());
    bases.push(format!("{casa}/.local/share/icons"));
    bases.push("/usr/share/icons".into());
    for b in &bases {
        for t in ["32x32", "48x48", "24x24", "64x64", "22x22", "128x128", "256x256"] {
            for sub in ["apps", "status", "panel"] {
                let c = PathBuf::from(format!("{b}/hicolor/{t}/{sub}/{nome}.png"));
                if c.exists() {
                    return Some(c);
                }
            }
        }
        let c = PathBuf::from(format!("{b}/{nome}.png"));
        if c.exists() {
            return Some(c);
        }
    }
    None
}

fn limpar_texto(s: &str) -> String {
    s.chars().filter(|c| *c != '\t' && *c != '\n').collect()
}

/// Lê todos os itens e grava `itens`. Devolve o conteúdo (para comparar).
fn atualizar(con: &Connection, itens: &Itens, dbus: &DBusProxy) -> String {
    // Some quem saiu do barramento.
    {
        let mut v = itens.lock().unwrap();
        v.retain(|i| {
            zbus::names::BusName::try_from(i.servico.as_str())
                .ok()
                .and_then(|n| dbus.name_has_owner(n).ok())
                .unwrap_or(false)
        });
    }
    let lista = itens.lock().unwrap().clone();
    let mut texto = String::new();
    for (n, item) in lista.iter().enumerate() {
        let Some(p) = proxy(con, item) else { continue };
        let status: String = p.get_property("Status").unwrap_or_else(|_| "Active".into());
        if status == "Passive" {
            continue;
        }
        let titulo: String = p
            .get_property::<String>("Title")
            .ok()
            .filter(|t| !t.is_empty())
            .or_else(|| p.get_property::<String>("Id").ok())
            .unwrap_or_default();
        let arq = icone(&p, n).map(|c| c.to_string_lossy().to_string()).unwrap_or_default();
        texto += &format!("{n}\t{}\t{}\n", limpar_texto(&titulo), arq);
    }
    texto
}

/// Menu do item (com.canonical.dbusmenu), achatado num nível.
fn ler_menu(con: &Connection, item: &Item) -> Vec<(i32, String, String)> {
    let Some(p) = proxy(con, item) else { return Vec::new() };
    let Ok(caminho) = p.get_property::<OwnedObjectPath>("Menu") else { return Vec::new() };
    let Ok(menu) = Proxy::new(con, item.servico.as_str(), caminho.as_str(), "com.canonical.dbusmenu") else { return Vec::new() };
    let _: Result<bool, _> = menu.call("AboutToShow", &(0i32,));
    let vazio: Vec<&str> = Vec::new();
    let Ok((_, (_, _, filhos))): Result<(u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)), _> =
        menu.call("GetLayout", &(0i32, -1i32, vazio))
    else {
        return Vec::new();
    };
    let mut saida = Vec::new();
    type No = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);
    fn percorrer(filhos: &[OwnedValue], saida: &mut Vec<(i32, String, String)>, nivel: usize) {
        for f in filhos {
            let Ok(copia) = f.try_clone() else { continue };
            let Ok((id, props, netos)) = <No>::try_from(copia) else { continue };
            let pegar = |k: &str| -> Option<&Value<'_>> {
                let v: &Value = props.get(k)?;
                Some(match v {
                    Value::Value(inner) => inner,
                    outro => outro,
                })
            };
            if matches!(pegar("visible"), Some(Value::Bool(false))) {
                continue;
            }
            let mut flags = String::new();
            if matches!(pegar("type"), Some(Value::Str(t)) if t.as_str() == "separator") {
                flags.push('s');
            }
            if matches!(pegar("enabled"), Some(Value::Bool(false))) {
                flags.push('d');
            }
            if matches!(pegar("toggle-state"), Some(Value::I32(1))) {
                flags.push('m');
            }
            let rotulo = match pegar("label") {
                Some(Value::Str(t)) => t.as_str().replace('_', ""),
                _ => String::new(),
            };
            let sub = matches!(pegar("children-display"), Some(Value::Str(t)) if t.as_str() == "submenu");
            if sub && nivel < 2 {
                // Submenu: o título vira um separador com nome, e os itens vêm depois.
                saida.push((id, limpar_texto(&rotulo), "sd".into()));
                percorrer(&netos, saida, nivel + 1);
                continue;
            }
            if rotulo.is_empty() && !flags.contains('s') {
                continue;
            }
            saida.push((id, limpar_texto(&rotulo), flags));
        }
    }
    percorrer(&filhos, &mut saida, 0);
    saida
}

fn atender(con: &Connection, itens: &Itens, linha: &str) {
    let partes: Vec<&str> = linha.split_whitespace().collect();
    let item = |n: &str| -> Option<Item> {
        let n: usize = n.parse().ok()?;
        itens.lock().unwrap().get(n).cloned()
    };
    match partes.as_slice() {
        ["ativar", n, x, y] | ["secundario", n, x, y] => {
            let Some(it) = item(n) else { return };
            let (x, y): (i32, i32) = (x.parse().unwrap_or(0), y.parse().unwrap_or(0));
            if let Some(p) = proxy(con, &it) {
                let metodo = if partes[0] == "ativar" { "Activate" } else { "SecondaryActivate" };
                let r: zbus::Result<()> = p.call(metodo, &(x, y));
                // Sem Activate (só menu): abre o menu.
                if r.is_err() && partes[0] == "ativar" {
                    atender(con, itens, &format!("menu {n}"));
                }
            }
        }
        ["menu", n] => {
            let Some(it) = item(n) else { return };
            let menu = ler_menu(con, &it);
            let mut texto = String::new();
            for (id, rotulo, flags) in menu.iter().take(40) {
                texto += &format!("{id}\t{rotulo}\t{flags}\n");
            }
            let arq = pasta().join(format!("menu-{n}"));
            if std::fs::write(&arq, texto).is_ok() {
                avisar_plumagem(&format!("bandeja-menu {n}"));
            }
        }
        ["clicar", n, id] => {
            let Some(it) = item(n) else { return };
            let Some(p) = proxy(con, &it) else { return };
            let Ok(caminho) = p.get_property::<OwnedObjectPath>("Menu") else { return };
            if let Ok(menu) = Proxy::new(con, it.servico.as_str(), caminho.as_str(), "com.canonical.dbusmenu") {
                let id: i32 = id.parse().unwrap_or(0);
                let dado = zbus::zvariant::Value::from(0i32);
                let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0);
                let _: zbus::Result<()> = menu.call("Event", &(id, "clicked", dado, agora));
            }
        }
        _ => {}
    }
}

extern "C" {
    fn mkfifo(caminho: *const std::os::raw::c_char, modo: u32) -> i32;
}

fn main() -> zbus::Result<()> {
    let dir = pasta();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| zbus::Error::Failure(e.to_string()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let itens: Itens = Arc::new(Mutex::new(Vec::new()));
    let con = zbus::blocking::connection::Builder::session()?
        .serve_at("/StatusNotifierWatcher", Vigia { itens: itens.clone() })?
        .name("org.kde.StatusNotifierWatcher")?
        .build()?;
    let dbus = DBusProxy::new(&con)?;

    // Pedidos da Plumagem.
    let ctl = dir.join("ctl");
    if let Ok(c) = std::ffi::CString::new(ctl.to_string_lossy().as_bytes()) {
        // SAFETY: string C válida durante a chamada.
        unsafe { mkfifo(c.as_ptr(), 0o600) };
    }
    {
        let con = con.clone();
        let itens = itens.clone();
        std::thread::spawn(move || loop {
            // Abrir para leitura espera um escritor; cada escritor manda linhas.
            let Ok(f) = std::fs::File::open(&ctl) else {
                std::thread::sleep(Duration::from_secs(1));
                continue;
            };
            for linha in BufReader::new(f).lines().map_while(Result::ok) {
                atender(&con, &itens, linha.trim());
            }
        });
    }

    // Lê os itens; só regrava e avisa quando algo mudou. Com a bandeja
    // vazia, confere raramente.
    let mut antes = String::from("-");
    loop {
        let agora = atualizar(&con, &itens, &dbus);
        if agora != antes {
            let arq = dir.join("itens");
            let tmp = dir.join("itens.tmp");
            if std::fs::write(&tmp, &agora).is_ok() && std::fs::rename(&tmp, &arq).is_ok() {
                avisar_plumagem("bandeja");
            }
            antes = agora;
        }
        let vazio = itens.lock().unwrap().is_empty();
        std::thread::sleep(Duration::from_millis(if vazio { 1000 } else { 2000 }));
    }
}
