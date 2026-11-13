// SPDX-License-Identifier: GPL-3.0-or-later
//! xdg-desktop-portal-ararinha: a parte do Ararinha nos portais do Flatpak e
//! o servidor de notificações da sessão.
//!
//! - FileChooser: "Abrir" e "Salvar como" pelo Explorador (`--escolher`).
//! - Settings: avisa os apps se o tema é claro ou escuro.
//! - Notification (portal) e org.freedesktop.Notifications: viram as
//!   notificações da Plumagem (pasta $XDG_RUNTIME_DIR/ararinha-avisos).
//!
//! - ScreenCast: compartilhar a tela (OBS, Discord, navegador). Pergunta à
//!   pessoa pela Ferramenta de Captura (`captura --permitir`) e roda o
//!   ajudante `ararinha-tela-pw`, que entrega a tela ao PipeWire.
//!
//! É ativado pelo D-Bus de sessão quando alguém precisa dele.

use std::{
    collections::HashMap,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use zbus::{
    fdo, interface,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

// ---------------------------------------------------------------- ajuda

fn opcao_bool(op: &HashMap<String, OwnedValue>, chave: &str) -> bool {
    match op.get(chave).map(|v| &**v) {
        Some(Value::Bool(b)) => *b,
        _ => false,
    }
}

fn opcao_texto(op: &HashMap<String, OwnedValue>, chave: &str) -> Option<String> {
    match op.get(chave).map(|v| &**v) {
        Some(Value::Str(s)) => Some(s.as_str().to_string()),
        _ => None,
    }
}

/// Opção do tipo "ay" (caminho terminado em NUL).
fn opcao_bytes(op: &HashMap<String, OwnedValue>, chave: &str) -> Option<String> {
    match op.get(chave).map(|v| &**v) {
        Some(Value::Array(a)) => {
            let bytes: Vec<u8> = a
                .iter()
                .filter_map(|x| match x {
                    Value::U8(b) => Some(*b),
                    _ => None,
                })
                .take_while(|b| *b != 0)
                .collect();
            String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
        }
        _ => None,
    }
}

/// file:// com os caracteres especiais escapados.
fn uri_de(p: &str) -> String {
    let mut s = String::from("file://");
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

fn valor<T: Into<Value<'static>>>(v: T) -> OwnedValue {
    OwnedValue::try_from(v.into()).expect("valor sem descritor de arquivo")
}

/// Abre o Explorador no modo escolher e espera, sem travar o D-Bus.
async fn escolher(args: Vec<String>) -> Option<Vec<String>> {
    let mut filho = Command::new("explorador")
        .arg("--escolher")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    loop {
        match filho.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                async_io::Timer::after(Duration::from_millis(100)).await;
            }
            Err(_) => return None,
        }
    }
    let saida = filho.wait_with_output().ok()?;
    let linhas: Vec<String> =
        String::from_utf8_lossy(&saida.stdout).lines().filter(|l| !l.is_empty()).map(String::from).collect();
    (!linhas.is_empty()).then_some(linhas)
}

fn resposta(caminhos: Option<Vec<String>>) -> (u32, HashMap<String, OwnedValue>) {
    match caminhos {
        Some(c) => {
            let uris: Vec<String> = c.iter().map(|p| uri_de(p)).collect();
            let mut r = HashMap::new();
            r.insert("uris".to_string(), valor(uris));
            (0, r)
        }
        // 1 = o usuário cancelou.
        None => (1, HashMap::new()),
    }
}

// ------------------------------------------------------------ FileChooser

struct Escolhedor;

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl Escolhedor {
    async fn open_file(
        &self,
        _handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let modo = if opcao_bool(&options, "directory") { "pasta" } else { "abrir" };
        let mut args = vec![modo.to_string(), "--titulo".into(), title];
        if opcao_bool(&options, "multiple") {
            args.push("--varios".into());
        }
        if let Some(p) = opcao_bytes(&options, "current_folder") {
            args.push("--pasta".into());
            args.push(p);
        }
        resposta(escolher(args).await)
    }

    async fn save_file(
        &self,
        _handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let mut args = vec!["salvar".to_string(), "--titulo".into(), title];
        if let Some(n) = opcao_texto(&options, "current_name") {
            args.push("--nome".into());
            args.push(n);
        }
        let pasta = opcao_bytes(&options, "current_folder").or_else(|| {
            // Sem pasta sugerida: Downloads, como o Windows.
            std::env::var("HOME").ok().map(|h| format!("{h}/Downloads")).filter(|p| PathBuf::from(p).is_dir())
        });
        if let Some(p) = pasta {
            args.push("--pasta".into());
            args.push(p);
        }
        resposta(escolher(args).await)
    }
}

// ---------------------------------------------------------------- Settings

fn tema_escuro() -> bool {
    let casa = std::env::var("HOME").unwrap_or_default();
    for c in [format!("{casa}/.config/plumagem/plumagem.conf"), "/etc/plumagem/plumagem.conf".into()] {
        let Ok(t) = std::fs::read_to_string(&c) else { continue };
        for l in t.lines() {
            if l.trim_start().starts_with('#') {
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == "tema" {
                    return v.trim().trim_matches('"') == "escuro";
                }
            }
        }
    }
    false
}

struct Config;

impl Config {
    fn aparencia() -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        // 0 sem preferência, 1 escuro, 2 claro.
        m.insert("color-scheme".to_string(), valor(if tema_escuro() { 1u32 } else { 2u32 }));
        // Azul do Ararinha (#2B7BC4) em RGB de 0 a 1.
        m.insert("accent-color".to_string(), valor((43.0f64 / 255.0, 123.0f64 / 255.0, 196.0f64 / 255.0)));
        m
    }

    /// Botões da barra de título para apps que desenham a própria (Chromium,
    /// apps GTK/Electron): minimizar, maximizar e fechar à direita, como no
    /// Windows. Sem isso, o GTK usa o padrão do GNOME (só fechar).
    fn janelas() -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        m.insert("button-layout".to_string(), valor(":minimize,maximize,close".to_string()));
        m
    }

    fn interface() -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        m.insert("color-scheme".to_string(), valor(if tema_escuro() { "prefer-dark".to_string() } else { "default".to_string() }));
        m.insert("font-name".to_string(), valor("Inter 10".to_string()));
        m
    }

    fn todos() -> Vec<(&'static str, HashMap<String, OwnedValue>)> {
        vec![
            ("org.freedesktop.appearance", Config::aparencia()),
            ("org.gnome.desktop.wm.preferences", Config::janelas()),
            ("org.gnome.desktop.interface", Config::interface()),
        ]
    }
}

#[interface(name = "org.freedesktop.impl.portal.Settings")]
impl Config {
    fn read_all(&self, namespaces: Vec<String>) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut r = HashMap::new();
        let quer = |ns: &str| {
            namespaces.is_empty()
                || namespaces.iter().any(|n| n == ns || (n.ends_with('*') && ns.starts_with(n.trim_end_matches('*'))))
        };
        for (ns, valores) in Config::todos() {
            if quer(ns) {
                r.insert(ns.to_string(), valores);
            }
        }
        r
    }

    fn read(&self, namespace: String, key: String) -> fdo::Result<OwnedValue> {
        for (ns, mut valores) in Config::todos() {
            if ns == namespace {
                if let Some(v) = valores.remove(&key) {
                    return Ok(v);
                }
            }
        }
        Err(fdo::Error::Failed(format!("configuração desconhecida: {namespace} {key}")))
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        2
    }
}

// ---------------------------------------------------------- Notificações

fn mostrar_aviso(titulo: &str, texto: &str) {
    let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let pasta = PathBuf::from(base).join("ararinha-avisos");
    let _ = std::fs::create_dir_all(&pasta);
    static N: AtomicU32 = AtomicU32::new(0);
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let nome = format!("{agora}-{}", N.fetch_add(1, Ordering::Relaxed));
    let tmp = pasta.join(format!(".{nome}.tmp"));
    // Uma linha só para o texto (a Plumagem junta as linhas).
    let texto = texto.replace('\n', " ");
    if std::fs::write(&tmp, format!("{}\n{}\n", titulo.replace('\n', " "), texto)).is_ok() {
        let _ = std::fs::rename(&tmp, pasta.join(format!("{nome}.txt")));
    }
}

struct AvisosPortal;

#[interface(name = "org.freedesktop.impl.portal.Notification")]
impl AvisosPortal {
    fn add_notification(&self, _app_id: String, _id: String, notification: HashMap<String, OwnedValue>) {
        let titulo = opcao_texto(&notification, "title").unwrap_or_default();
        let corpo = opcao_texto(&notification, "body").unwrap_or_default();
        if !titulo.is_empty() || !corpo.is_empty() {
            mostrar_aviso(if titulo.is_empty() { "Notificação" } else { &titulo }, &corpo);
        }
    }

    fn remove_notification(&self, _app_id: String, _id: String) {}
}

struct Notificacoes {
    proximo: AtomicU32,
}

#[interface(name = "org.freedesktop.Notifications")]
impl Notificacoes {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        _hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let titulo = if summary.is_empty() { app_name } else { summary };
        // Tira as marcações simples (<b>, <i>) que alguns apps mandam.
        let mut limpo = String::new();
        let mut dentro = false;
        for c in body.chars() {
            match c {
                '<' => dentro = true,
                '>' => dentro = false,
                _ if !dentro => limpo.push(c),
                _ => {}
            }
        }
        mostrar_aviso(&titulo, &limpo);
        if replaces_id != 0 {
            replaces_id
        } else {
            self.proximo.fetch_add(1, Ordering::Relaxed)
        }
    }

    fn close_notification(&self, _id: u32) {}

    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("Plumagem".into(), "Ararinha Azul Linux".into(), env!("CARGO_PKG_VERSION").into(), "1.2".into())
    }
}

// ------------------------------------------------------------ AppChooser

/// org.freedesktop.impl.portal.AppChooser: "abrir o arquivo/pasta" de um
/// app Flatpak (Chromium: abrir o download, mostrar na pasta). Sem ele o
/// portal fica esperando e o app parece travado. Escolhe sem perguntar:
/// o padrão do mimeapps.list, o Explorador para pastas, ou o primeiro.
struct EscolhaApp;

fn padrao_mimeapps(tipo: &str) -> Option<String> {
    let casa = std::env::var("HOME").ok()?;
    let t = std::fs::read_to_string(format!("{casa}/.config/mimeapps.list")).ok()?;
    let mut na = false;
    for l in t.lines() {
        if l.starts_with('[') {
            na = l.trim() == "[Default Applications]";
            continue;
        }
        if let (true, Some((k, v))) = (na, l.split_once('=')) {
            if k.trim() == tipo {
                return v.split(';').next().map(|s| s.trim().trim_end_matches(".desktop").to_string()).filter(|s| !s.is_empty());
            }
        }
    }
    None
}

#[interface(name = "org.freedesktop.impl.portal.AppChooser")]
impl EscolhaApp {
    async fn choose_application(
        &self,
        _handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        choices: Vec<String>,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let tipo = options
            .get("content_type")
            .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
            .unwrap_or_default();
        let escolha = padrao_mimeapps(&tipo)
            .filter(|p| choices.contains(p))
            .or_else(|| (tipo == "inode/directory" && choices.iter().any(|c| c == "explorador")).then(|| "explorador".to_string()))
            .or_else(|| choices.first().cloned());
        match escolha {
            Some(c) => {
                let mut r = HashMap::new();
                r.insert("choice".to_string(), valor(c));
                (0, r)
            }
            // Nenhum app abre esse tipo: 2 = erro (o app mostra a mensagem).
            None => (2, HashMap::new()),
        }
    }

    fn update_choices(&self, _handle: OwnedObjectPath, _choices: Vec<String>) {}
}

// ------------------------------------------------------------ FileManager1

/// org.freedesktop.FileManager1: "Mostrar na pasta" dos apps (o portal
/// OpenDirectory chama este). Abre o Explorador com o item selecionado.
struct GerenciadorArquivos;

fn abrir_explorador(uris: &[String]) {
    for u in uris.iter().take(4) {
        let _ = Command::new("explorador").arg(u).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
}

#[interface(name = "org.freedesktop.FileManager1")]
impl GerenciadorArquivos {
    fn show_folders(&self, uris: Vec<String>, _startup_id: String) {
        abrir_explorador(&uris);
    }

    fn show_items(&self, uris: Vec<String>, _startup_id: String) {
        abrir_explorador(&uris);
    }

    fn show_item_properties(&self, uris: Vec<String>, _startup_id: String) {
        abrir_explorador(&uris);
    }
}

// ------------------------------------------------------------ ScreenCast

type Ajudantes = std::sync::Arc<std::sync::Mutex<HashMap<String, std::process::Child>>>;

struct Transmissao {
    ajudantes: Ajudantes,
}

/// Sessão de compartilhamento (o portal fecha quando o app para).
struct Sessao {
    caminho: String,
    ajudantes: Ajudantes,
}

#[interface(name = "org.freedesktop.impl.portal.Session")]
impl Sessao {
    fn close(&self) {
        if let Ok(mut m) = self.ajudantes.lock() {
            if let Some(mut c) = m.remove(&self.caminho) {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }

    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }
}

/// "com.discordapp.Discord" → "Discord".
fn nome_do_app(app_id: &str) -> String {
    let n = app_id.rsplit('.').next().unwrap_or("").trim();
    if n.is_empty() {
        "Um aplicativo".into()
    } else {
        n.to_string()
    }
}

/// Pergunta à pessoa (janela da Ferramenta de Captura) sem travar o D-Bus.
async fn permitir(app: &str) -> bool {
    let Ok(mut filho) = Command::new("captura").args(["--permitir", app]).stdin(Stdio::null()).spawn() else { return false };
    loop {
        match filho.try_wait() {
            Ok(Some(st)) => return st.success(),
            Ok(None) => async_io::Timer::after(Duration::from_millis(100)).await,
            Err(_) => return false,
        }
    }
}

/// Liga o ajudante do PipeWire e espera ele dizer o nó e o tamanho.
async fn ligar_ajudante() -> Option<(std::process::Child, u32, i32, i32)> {
    let mut filho = Command::new("/usr/libexec/ararinha-tela-pw")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .ok()?;
    let saida = filho.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut linha = String::new();
        let _ = std::io::BufReader::new(saida).read_line(&mut linha);
        let _ = tx.send(linha);
    });
    for _ in 0..100 {
        match rx.try_recv() {
            Ok(l) => {
                let v: Vec<&str> = l.split_whitespace().collect();
                if let [no, w, h] = v[..] {
                    if let (Ok(no), Ok(w), Ok(h)) = (no.parse(), w.parse(), h.parse()) {
                        return Some((filho, no, w, h));
                    }
                }
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => async_io::Timer::after(Duration::from_millis(100)).await,
            Err(_) => break,
        }
    }
    let _ = filho.kill();
    let _ = filho.wait();
    None
}

#[interface(name = "org.freedesktop.impl.portal.ScreenCast")]
impl Transmissao {
    async fn create_session(
        &self,
        _handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        _app_id: String,
        _options: HashMap<String, OwnedValue>,
        #[zbus(object_server)] servidor: &zbus::ObjectServer,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let s = Sessao { caminho: session_handle.as_str().to_string(), ajudantes: self.ajudantes.clone() };
        match servidor.at(session_handle.as_ref(), s).await {
            Ok(_) => (0, HashMap::new()),
            Err(_) => (2, HashMap::new()),
        }
    }

    async fn select_sources(
        &self,
        _handle: OwnedObjectPath,
        _session_handle: OwnedObjectPath,
        _app_id: String,
        _options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        // Só a tela inteira por enquanto: a escolha acontece no Start.
        (0, HashMap::new())
    }

    async fn start(
        &self,
        _handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        app_id: String,
        _parent_window: String,
        _options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        if !permitir(&nome_do_app(&app_id)).await {
            return (1, HashMap::new());
        }
        let Some((filho, no, w, h)) = ligar_ajudante().await else { return (2, HashMap::new()) };
        if let Ok(mut m) = self.ajudantes.lock() {
            if let Some(mut antigo) = m.insert(session_handle.as_str().to_string(), filho) {
                let _ = antigo.kill();
                let _ = antigo.wait();
            }
        }
        let mut props: HashMap<String, OwnedValue> = HashMap::new();
        props.insert("size".into(), valor((w, h)));
        props.insert("position".into(), valor((0i32, 0i32)));
        props.insert("source_type".into(), valor(1u32));
        let fluxos: Vec<(u32, HashMap<String, OwnedValue>)> = vec![(no, props)];
        let mut r = HashMap::new();
        r.insert("streams".to_string(), valor(fluxos));
        (0, r)
    }

    /// 1 = monitor (tela inteira).
    #[zbus(property)]
    fn available_source_types(&self) -> u32 {
        1
    }

    /// 2 = cursor desenhado na imagem.
    #[zbus(property)]
    fn available_cursor_modes(&self) -> u32 {
        2
    }

    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        4
    }
}

fn main() -> zbus::Result<()> {
    let conexao = zbus::blocking::connection::Builder::session()?
        .serve_at("/org/freedesktop/portal/desktop", Escolhedor)?
        .serve_at("/org/freedesktop/portal/desktop", Config)?
        .serve_at("/org/freedesktop/portal/desktop", EscolhaApp)?
        .serve_at("/org/freedesktop/portal/desktop", AvisosPortal)?
        .serve_at("/org/freedesktop/portal/desktop", Transmissao { ajudantes: Default::default() })?
        .serve_at("/org/freedesktop/Notifications", Notificacoes { proximo: AtomicU32::new(1) })?
        .serve_at("/org/freedesktop/FileManager1", GerenciadorArquivos)?
        .build()?;
    // Os dois nomes; se outro processo já tiver um deles, fica com o outro.
    let mut algum = false;
    for nome in [
        "org.freedesktop.impl.portal.desktop.ararinha",
        "org.freedesktop.Notifications",
        "org.freedesktop.FileManager1",
    ] {
        match conexao.request_name(nome) {
            Ok(_) => algum = true,
            Err(e) => eprintln!("xdg-desktop-portal-ararinha: {nome}: {e}"),
        }
    }
    if !algum {
        return Ok(());
    }
    loop {
        std::thread::park();
    }
}
