#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// ponytail: renders via the OS webview (WebView2/WebKit) — a from-scratch engine is years of work and slower.
use std::{fs, path::PathBuf, process::Command, time::SystemTime};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    NewWindowResponse, PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder,
};

const BAR: f64 = 72.0;
const HOME: &str = "https://duckduckgo.com/";
const MAX_HISTORY: usize = 2000;

/// Shortcuts, injected into the toolbar and every page. Posts command names to Rust.
const KEYS: &str = r#"addEventListener('keydown',e=>{const k=e.key.toLowerCase(),c=e.ctrlKey||e.metaKey,s=e.shiftKey,a=e.altKey;
const m=c&&k=='tab'?(s?'prev':'next'):c&&s&&k=='b'?'panel:bookmarks':c&&!s&&!a&&/^[1-9]$/.test(k)?'nth:'+k:
c&&!s&&!a?{t:'new',w:'close',l:'focus-url',d:'star',h:'panel:history',j:'panel:downloads',r:'reload'}[k]:
a&&!c?{arrowleft:'back',arrowright:'fwd'}[k]:k=='f5'?'reload':0;
if(m){e.preventDefault();e.stopPropagation();window.ipc.postMessage(m)}},true);"#;

const TOOLBAR: &str = r#"<!doctype html><html><head><meta charset=utf-8><style>
:root{--bg:#dee1e6;--bar:#fff;--on:#fff;--fg:#1f1f1f;--mute:#5f6368;--field:#f1f3f4;--hov:rgba(0,0,0,.07);--acc:#e8590c}
@media(prefers-color-scheme:dark){:root{--bg:#1f2023;--bar:#35363a;--on:#35363a;--fg:#e8eaed;--mute:#9aa0a6;--field:#202124;--hov:rgba(255,255,255,.1)}}
*{box-sizing:border-box}
html,body{margin:0;height:100%;font:12px "Segoe UI",system-ui,sans-serif;color:var(--fg);background:var(--bar);overflow:hidden;user-select:none}
.i{font-family:"Segoe Fluent Icons","Segoe MDL2 Assets";border:0;background:none;color:var(--fg);width:30px;height:30px;border-radius:15px;font-size:14px;flex:none}
.i:hover{background:var(--hov)} .i:focus-visible,button:focus-visible{outline:2px solid var(--acc)}
#tabs{display:flex;gap:2px;height:36px;padding:6px 6px 0;background:var(--bg);align-items:flex-end}
.tab{display:flex;align-items:center;flex:0 1 220px;min-width:44px;height:30px;padding:0 4px 0 12px;border-radius:8px 8px 0 0;overflow:hidden}
.tab:hover{background:var(--hov)} .tab.on{background:var(--on)}
.tab span{flex:1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.tab .i{width:20px;height:20px;font-size:9px}
#nav{display:flex;align-items:center;gap:2px;height:36px;margin:0;padding:0 6px}
#u{flex:1;min-width:0;height:28px;border:0;border-radius:14px;padding:0 14px;background:var(--field);color:var(--fg);font:13px "Segoe UI",sans-serif;outline:none;user-select:text}
#u:focus{box-shadow:0 0 0 2px var(--acc)}
#star.on{color:var(--acc)}
#dl{position:relative} #dl[data-n]::after{content:attr(data-n);position:absolute;top:0;right:0;font:bold 9px "Segoe UI";background:var(--acc);color:#fff;border-radius:7px;padding:1px 4px}
#panel{position:absolute;inset:72px 0 0;overflow:auto;padding:20px max(16px,calc(50% - 380px));border-top:1px solid var(--hov)}
.ph{display:flex;align-items:center;gap:8px} h2{flex:1;margin:0;font:600 18px "Segoe UI",sans-serif}
#q{height:28px;width:220px;border:0;border-radius:14px;padding:0 12px;background:var(--field);color:var(--fg);outline:none;user-select:text}
#clr{height:28px;border:1px solid var(--hov);border-radius:14px;background:none;color:var(--fg);padding:0 12px}
ul{list-style:none;margin:12px 0;padding:0} li{display:flex;align-items:center;gap:8px;padding:8px 10px;border-radius:8px} li:hover{background:var(--hov)}
li div{flex:1;min-width:0} li b,li small{display:block;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;font-weight:400;font-size:13px} li small{color:var(--mute);font-size:11px}
.empty{color:var(--mute);padding:24px 10px}
</style></head><body>
<div id=tabs role=tablist></div>
<form id=nav>
<button type=button class=i id=bk title="Back (Alt+Left)" aria-label=Back>&#xE72B;</button>
<button type=button class=i id=fw title="Forward (Alt+Right)" aria-label=Forward>&#xE72A;</button>
<button type=button class=i id=rl title="Reload (F5)" aria-label=Reload>&#xE72C;</button>
<input id=u list=sug spellcheck=false autocomplete=off aria-label=Address placeholder="Search or enter address">
<button type=button class=i id=star title="Bookmark this page (Ctrl+D)" aria-label=Bookmark>&#xE734;</button>
<button type=button class=i data-p=bookmarks title="Bookmarks (Ctrl+Shift+B)" aria-label=Bookmarks>&#xE728;</button>
<button type=button class=i data-p=history title="History (Ctrl+H)" aria-label=History>&#xE81C;</button>
<button type=button class=i id=dl data-p=downloads title="Downloads (Ctrl+J)" aria-label=Downloads>&#xE896;</button>
</form>
<datalist id=sug></datalist>
<div id=panel hidden><div class=ph><h2 id=ph></h2><input id=q placeholder=Filter aria-label=Filter><button id=clr>Clear history</button><button class=i id=pc title="Close (Esc)" aria-label=Close>&#xE711;</button></div><ul id=list></ul></div>
<script>
const post=m=>window.ipc.postMessage(m),el=(t,c,x)=>{const e=document.createElement(t);if(c)e.className=c;if(x!=null)e.textContent=x;return e};let S={};
bk.onclick=()=>post('back');fw.onclick=()=>post('fwd');rl.onclick=()=>post('reload');star.onclick=()=>post('star');
nav.onsubmit=e=>{e.preventDefault();if(u.value.trim())post('go:'+u.value);u.blur()};
u.onfocus=()=>setTimeout(()=>u.select());
document.querySelectorAll('[data-p]').forEach(b=>b.onclick=()=>post('panel:'+b.dataset.p));
pc.onclick=()=>post('panel:');clr.onclick=()=>post('clear-history');q.oninput=()=>items();
addEventListener('keydown',e=>{if(e.key=='Escape'&&S.panel)post('panel:')});
function render(s){const was=S.panel&&S.panel.kind;S=s;tabs.textContent='';
s.tabs.forEach((t,i)=>{const d=el('div','tab'+(i==s.active?' on':'')),x=el('button','i','\uE711');d.role='tab';d.title=t.title+'\n'+t.url;
d.append(el('span',0,t.title||t.url||'New tab'),x);d.onclick=()=>post('tab:'+t.id);d.onauxclick=e=>{if(e.button==1)post('close:'+t.id)};
x.title='Close (Ctrl+W)';x.onclick=e=>{e.stopPropagation();post('close:'+t.id)};tabs.append(d)});
const p=el('button','i','\uE710');p.title='New tab (Ctrl+T)';p.onclick=()=>post('new');tabs.append(p);
if(document.activeElement!==u)u.value=s.url;
star.textContent=s.starred?'\uE735':'\uE734';star.classList.toggle('on',s.starred);
if(s.dl)dl.dataset.n=s.dl;else delete dl.dataset.n;
sug.textContent='';s.sug.forEach(v=>{const o=el('option');o.value=v;sug.append(o)});
panel.hidden=!s.panel;if(!s.panel)return;
if(was!=s.panel.kind){q.value='';setTimeout(()=>q.focus())}
ph.textContent={history:'History',bookmarks:'Bookmarks',downloads:'Downloads'}[s.panel.kind];
clr.hidden=s.panel.kind!='history';q.hidden=s.panel.kind=='downloads';items()}
function items(){const p=S.panel,f=q.value.toLowerCase();list.textContent='';let n=0;
p.items.forEach(([t,url,x],i)=>{if(n>=300||f&&!(t+' '+url).toLowerCase().includes(f))return;n++;
const li=el('li'),d=el('div');d.append(el('b',0,t||url),el('small',0,p.kind=='history'?new Date(x*1000).toLocaleString()+' \u00b7 '+url:p.kind=='downloads'?x+' \u00b7 '+url:url));li.append(d);
if(p.kind=='downloads'){li.onclick=()=>post('open-dl:'+i);const b=el('button','i','\uE838');b.title='Show in folder';b.onclick=e=>{e.stopPropagation();post('show-dl:'+i)};li.append(b)}
else{li.onclick=()=>post('go:'+url);if(p.kind=='bookmarks'){const b=el('button','i','\uE74D');b.title='Remove';b.onclick=e=>{e.stopPropagation();post('del-bm:'+url)};li.append(b)}}
list.append(li)});
if(!n)list.append(el('li','empty',p.kind=='downloads'?'No downloads this session':'Nothing here yet'))}
</script></body></html>"#;

enum Ev {
    Cmd(String, bool), // command, trusted (from toolbar) vs page shortcut
    Nav(u32, String),
    Title(u32, String),
    NewTab(String),
    DlStart(String, PathBuf),
    DlDone(String, Option<PathBuf>, bool),
}

struct Tab { id: u32, view: WebView, url: String, title: String }

/// A history or bookmark row, stored as `ts\turl\ttitle` lines.
struct Entry { ts: u64, url: String, title: String }

struct Download { url: String, path: PathBuf, status: &'static str }

struct App {
    window: Window,
    ctx: WebContext,
    proxy: EventLoopProxy<Ev>,
    toolbar: WebView,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u32,
    panel: Option<&'static str>,
    history: Vec<Entry>,
    bookmarks: Vec<Entry>,
    downloads: Vec<Download>,
    dir: PathBuf,
}

/// Turn whatever was typed into a URL: keep schemes, add https to hosts, search otherwise.
fn to_url(s: &str) -> String {
    let s = s.trim();
    if s.contains("://") || s.starts_with("about:") {
        s.into()
    } else if s.starts_with("localhost") || s.starts_with("127.0.0.1") {
        format!("http://{s}")
    } else if !s.contains(' ') && s.contains('.') {
        format!("https://{s}")
    } else {
        let q: String = s.bytes().map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        }).collect();
        format!("{HOME}?q={q}")
    }
}

/// Minimal JS/JSON string literal escape (avoids a serde_json dependency).
fn js(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '<' => o.push_str("\\u003c"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Commands a web page may trigger (keyboard shortcuts). Everything else must come from the toolbar.
fn page_allowed(m: &str) -> bool {
    matches!(m, "back" | "fwd" | "reload" | "new" | "close" | "next" | "prev" | "focus-url" | "star"
        | "panel:history" | "panel:bookmarks" | "panel:downloads")
        || m.strip_prefix("nth:").is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
}

fn rect(y: f64, w: f64, h: f64) -> Rect {
    Rect { position: LogicalPosition::new(0.0, y).into(), size: LogicalSize::new(w, h.max(0.0)).into() }
}

fn now() -> u64 { SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()) }

fn load(path: &PathBuf) -> Vec<Entry> {
    fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| {
        let mut p = l.splitn(3, '\t');
        Some(Entry { ts: p.next()?.parse().ok()?, url: p.next()?.into(), title: p.next().unwrap_or("").into() })
    }).collect()
}

/// Write via temp file + rename so a crash mid-write never truncates the file.
fn save(path: &PathBuf, rows: &[Entry]) {
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    let body: String = rows.iter().map(|e| format!("{}\t{}\t{}\n", e.ts, clean(&e.url), clean(&e.title))).collect();
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, body).is_ok() { let _ = fs::rename(tmp, path); }
}

fn shown_url(url: &str) -> &str { if url.starts_with("about:blank") { "" } else { url } }

impl App {
    fn size(&self) -> (f64, f64) {
        let s = self.window.inner_size().to_logical::<f64>(self.window.scale_factor());
        (s.width, s.height)
    }

    fn layout(&self) {
        let (w, h) = self.size();
        let _ = self.toolbar.set_bounds(rect(0.0, w, if self.panel.is_some() { h } else { BAR }));
        for (i, t) in self.tabs.iter().enumerate() {
            let _ = t.view.set_bounds(rect(BAR, w, h - BAR));
            let _ = t.view.set_visible(i == self.active && self.panel.is_none());
        }
    }

    fn new_tab(&mut self, url: &str) -> wry::Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        let (w, h) = self.size();
        let p = || self.proxy.clone();
        let (p1, p2, p3, p4, p5, p6) = (p(), p(), p(), p(), p(), p());
        let view = WebViewBuilder::new_with_web_context(&mut self.ctx)
            .with_bounds(rect(BAR, w, h - BAR))
            .with_url(url)
            .with_initialization_script(KEYS)
            .with_ipc_handler(move |r| { let _ = p1.send_event(Ev::Cmd(r.into_body(), false)); })
            .with_on_page_load_handler(move |e, url| {
                if let PageLoadEvent::Started = e { let _ = p2.send_event(Ev::Nav(id, url)); }
            })
            .with_document_title_changed_handler(move |t| { let _ = p3.send_event(Ev::Title(id, t)); })
            .with_new_window_req_handler(move |url, _| {
                let _ = p4.send_event(Ev::NewTab(url));
                NewWindowResponse::Deny
            })
            .with_download_started_handler(move |url, path| {
                let _ = p5.send_event(Ev::DlStart(url, path.clone()));
                true
            })
            .with_download_completed_handler(move |url, path, ok| { let _ = p6.send_event(Ev::DlDone(url, path, ok)); })
            .build_as_child(&self.window)?;
        self.tabs.push(Tab { id, view, url: url.into(), title: String::new() });
        self.select(self.tabs.len() - 1);
        Ok(())
    }

    fn select(&mut self, i: usize) {
        self.active = i.min(self.tabs.len() - 1);
        self.panel = None;
        self.layout();
        let _ = self.tabs[self.active].view.focus();
    }

    /// Returns false when the last tab closed (quit).
    fn close_tab(&mut self, i: usize) -> bool {
        if i >= self.tabs.len() { return true; }
        self.tabs.remove(i);
        if self.tabs.is_empty() { return false; }
        if i < self.active || self.active >= self.tabs.len() { self.active -= 1; }
        self.select(self.active);
        true
    }

    fn idx(&self, id: &str) -> Option<usize> {
        let id: u32 = id.parse().ok()?;
        self.tabs.iter().position(|t| t.id == id)
    }

    fn focus_url(&self) {
        let _ = self.toolbar.focus();
        let _ = self.toolbar.evaluate_script("u.focus()");
    }

    fn file(&self, name: &str) -> PathBuf { self.dir.join(name) }

    /// Returns false to quit.
    fn cmd(&mut self, m: &str, trusted: bool) -> bool {
        if !trusted && !page_allowed(m) { return true; }
        let (c, arg) = m.split_once(':').unwrap_or((m, ""));
        let v = &self.tabs[self.active].view;
        match c {
            "back" => { let _ = v.evaluate_script("history.back()"); }
            "fwd" => { let _ = v.evaluate_script("history.forward()"); }
            "reload" => { let _ = v.reload(); }
            "go" => {
                let _ = v.load_url(&to_url(arg));
                self.panel = None;
                self.layout();
                let _ = self.tabs[self.active].view.focus();
            }
            "new" => { let _ = self.new_tab("about:blank"); self.focus_url(); }
            "close" => {
                let i = if arg.is_empty() { Some(self.active) } else { self.idx(arg) };
                if let Some(i) = i { if !self.close_tab(i) { return false; } }
            }
            "tab" => { if let Some(i) = self.idx(arg) { self.select(i); } }
            "next" => self.select((self.active + 1) % self.tabs.len()),
            "prev" => self.select((self.active + self.tabs.len() - 1) % self.tabs.len()),
            "nth" => {
                let n: usize = arg.parse().unwrap_or(1);
                self.select(if n == 9 { self.tabs.len() - 1 } else { n - 1 });
            }
            "focus-url" => self.focus_url(),
            "star" => {
                let t = &self.tabs[self.active];
                if shown_url(&t.url).is_empty() { return true; }
                if let Some(i) = self.bookmarks.iter().position(|b| b.url == t.url) {
                    self.bookmarks.remove(i);
                } else {
                    self.bookmarks.push(Entry { ts: now(), url: t.url.clone(), title: t.title.clone() });
                }
                save(&self.file("bookmarks.tsv"), &self.bookmarks);
            }
            "panel" => {
                let want = match arg { "history" => Some("history"), "bookmarks" => Some("bookmarks"), "downloads" => Some("downloads"), _ => None };
                self.panel = if want == self.panel { None } else { want };
                self.layout();
                if self.panel.is_some() { let _ = self.toolbar.focus(); } else { let _ = self.tabs[self.active].view.focus(); }
            }
            "del-bm" => {
                self.bookmarks.retain(|b| b.url != arg);
                save(&self.file("bookmarks.tsv"), &self.bookmarks);
            }
            "clear-history" => {
                self.history.clear();
                save(&self.file("history.tsv"), &self.history);
            }
            "open-dl" | "show-dl" => {
                if let Some(d) = arg.parse::<usize>().ok().and_then(|i| self.downloads.get(i)) {
                    let mut e = Command::new("explorer");
                    if c == "show-dl" {
                        use std::os::windows::process::CommandExt;
                        e.raw_arg(format!("/select,\"{}\"", d.path.display()));
                    } else {
                        e.arg(&d.path);
                    }
                    let _ = e.spawn();
                }
            }
            _ => {}
        }
        self.render();
        true
    }

    fn on_nav(&mut self, id: u32, url: String) {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else { return };
        self.tabs[i].url = url.clone();
        if url.starts_with("http") && self.history.last().is_none_or(|h| h.url != url) {
            self.history.push(Entry { ts: now(), url, title: String::new() });
            let over = self.history.len().saturating_sub(MAX_HISTORY);
            self.history.drain(..over);
            save(&self.file("history.tsv"), &self.history);
        }
        self.render();
    }

    fn on_title(&mut self, id: u32, title: String) {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else { return };
        let url = self.tabs[i].url.clone();
        if let Some(h) = self.history.iter_mut().rev().take(20).find(|h| h.url == url) {
            if h.title != title {
                h.title = title.clone();
                save(&self.file("history.tsv"), &self.history);
            }
        }
        self.tabs[i].title = title;
        self.render();
    }

    fn render(&self) {
        let t = &self.tabs[self.active];
        let title = if t.title.is_empty() { "Ferrite".into() } else { format!("{} - Ferrite", t.title) };
        self.window.set_title(&title);

        let tabs: Vec<String> = self.tabs.iter()
            .map(|t| format!("{{\"id\":{},\"title\":{},\"url\":{}}}", t.id, js(&t.title), js(shown_url(&t.url))))
            .collect();
        // ponytail: suggestions resent on every render; fine at a few hundred entries.
        let mut sug: Vec<&str> = self.bookmarks.iter().rev().chain(self.history.iter().rev()).map(|e| e.url.as_str()).collect();
        let mut seen = std::collections::HashSet::new();
        sug.retain(|u| seen.insert(*u));
        sug.truncate(300);
        let sug: Vec<String> = sug.into_iter().map(js).collect();
        let row = |t: &str, u: &str, x: &str| format!("[{},{},{}]", js(t), js(u), js(x));
        let panel = match self.panel {
            None => "null".into(),
            Some(k) => {
                let rows: Vec<String> = match k {
                    "history" => self.history.iter().rev().map(|e| row(&e.title, &e.url, &e.ts.to_string())).collect(),
                    "bookmarks" => self.bookmarks.iter().rev().map(|e| row(&e.title, &e.url, "")).collect(),
                    _ => self.downloads.iter().map(|d| {
                        let name = d.path.file_name().map_or(d.url.clone(), |n| n.to_string_lossy().into_owned());
                        row(&name, &d.url, d.status)
                    }).collect(),
                };
                format!("{{\"kind\":{},\"items\":[{}]}}", js(k), rows.join(","))
            }
        };
        let busy = self.downloads.iter().filter(|d| d.status == "Downloading").count();
        let starred = self.bookmarks.iter().any(|b| b.url == t.url);
        let _ = self.toolbar.evaluate_script(&format!(
            "render({{\"tabs\":[{}],\"active\":{},\"url\":{},\"starred\":{starred},\"dl\":{busy},\"sug\":[{}],\"panel\":{panel}}})",
            tabs.join(","), self.active, js(shown_url(&t.url)), sug.join(","),
        ));
    }
}

fn main() -> wry::Result<()> {
    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let mut wb = WindowBuilder::new()
        .with_title("Ferrite")
        .with_inner_size(LogicalSize::new(1280.0, 850.0));
    #[cfg(windows)]
    {
        use tao::platform::windows::IconExtWindows;
        wb = wb.with_window_icon(tao::window::Icon::from_resource(1, None).ok());
    }
    let window = wb.build(&event_loop).unwrap();

    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default();
    let dir = base.join("Ferrite");
    let _ = fs::create_dir_all(&dir);
    let mut ctx = WebContext::new(Some(dir.join("webview")));

    let p = event_loop.create_proxy();
    let w = window.inner_size().to_logical::<f64>(window.scale_factor()).width;
    let toolbar = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_bounds(rect(0.0, w, BAR))
        .with_html(format!("{TOOLBAR}<script>{KEYS}</script>"))
        .with_ipc_handler(move |r| { let _ = p.send_event(Ev::Cmd(r.into_body(), true)); })
        .build_as_child(&window)?;

    let mut app = App {
        history: load(&dir.join("history.tsv")),
        bookmarks: load(&dir.join("bookmarks.tsv")),
        window, ctx, toolbar, dir,
        proxy: event_loop.create_proxy(),
        tabs: vec![], active: 0, next_id: 0, panel: None, downloads: vec![],
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { app.new_tab(HOME)?; }
    for a in &args { app.new_tab(&to_url(a))?; }
    app.select(0);
    app.render();

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let keep = match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => false,
            Event::WindowEvent { event: WindowEvent::Resized(_), .. } => { app.layout(); true }
            Event::UserEvent(Ev::Cmd(m, trusted)) => app.cmd(&m, trusted),
            Event::UserEvent(Ev::Nav(id, url)) => { app.on_nav(id, url); true }
            Event::UserEvent(Ev::Title(id, t)) => { app.on_title(id, t); true }
            Event::UserEvent(Ev::NewTab(url)) => { let _ = app.new_tab(&url); app.render(); true }
            Event::UserEvent(Ev::DlStart(url, path)) => {
                app.downloads.push(Download { url, path, status: "Downloading" });
                app.render();
                true
            }
            Event::UserEvent(Ev::DlDone(url, path, ok)) => {
                if let Some(d) = app.downloads.iter_mut().rev().find(|d| d.url == url && d.status == "Downloading") {
                    d.status = if ok { "Done" } else { "Failed" };
                    if let Some(p) = path { d.path = p; }
                }
                app.render();
                true
            }
            _ => true,
        };
        if !keep { *flow = ControlFlow::Exit; }
    });
}

#[test]
fn urls() {
    assert_eq!(to_url("example.com"), "https://example.com");
    assert_eq!(to_url("http://x.y/a"), "http://x.y/a");
    assert_eq!(to_url("localhost:3000"), "http://localhost:3000");
    assert_eq!(to_url("rust lang"), "https://duckduckgo.com/?q=rust+lang");
    assert_eq!(to_url("a&b"), "https://duckduckgo.com/?q=a%26b");
    assert_eq!(js("a\"</script>\n"), "\"a\\\"\\u003c/script>\\u000a\"");
    assert!(page_allowed("nth:3") && page_allowed("panel:history"));
    assert!(!page_allowed("open-dl:0") && !page_allowed("go:file:///c:/") && !page_allowed("nth:10") && !page_allowed("clear-history"));
}

#[test]
fn history_roundtrip() {
    let p = std::env::temp_dir().join("ferrite_test_history.tsv");
    save(&p, &[Entry { ts: 5, url: "https://a.b/".into(), title: "x\ty".into() }]);
    let back = load(&p);
    assert_eq!((back[0].ts, back[0].url.as_str(), back[0].title.as_str()), (5, "https://a.b/", "x y"));
    let _ = fs::remove_file(p);
}
