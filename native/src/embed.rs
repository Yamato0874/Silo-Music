//! Официальные встроенные плееры (YouTube IFrame API и виджет SoundCloud) в маленьком окне WebView2.
//! Создаётся только когда играет такой трек и уничтожается после: остальное время веб-движка в памяти нет.
//! Караоке в этом режиме рисуется самой страницей поверх клипа (родное окно не может рисовать поверх WebView2).
use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use wry::dpi::{LogicalPosition, LogicalSize};
use wry::http::{Request, Response};
use wry::{Rect, WebContext, WebView, WebViewBuilder};

#[derive(Default, Clone, Debug)]
pub struct EmbedState {
    pub playing: bool,
    pub pos: f64,
    pub dur: f64,
    pub ended: bool,
    pub error: Option<String>,
    pub ready: bool,
}

pub struct Embed {
    view: WebView,
    /// скрипты ждут, пока загрузится страница плеера (до этого WebView2 их теряет)
    queue: Vec<String>,
    booted: Arc<std::sync::atomic::AtomicBool>,
    _ctx: WebContext,
    pub st: Arc<Mutex<EmbedState>>,
    pub key: String,
}

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;height:100%;background:#000;overflow:hidden;font-family:"Segoe UI",sans-serif}
#p,#p iframe{position:absolute;inset:0;width:100%;height:100%;border:0}
#k{position:absolute;left:0;right:0;bottom:0;top:0;pointer-events:none;display:none;overflow:hidden;
 background:linear-gradient(transparent 0%,rgba(0,0,0,.25) 40%,rgba(0,0,0,.65) 100%)}
#kl{position:absolute;left:6%;right:6%;top:50%;transition:transform .45s ease-out;text-align:center}
#kl div{color:rgba(255,255,255,.55);font-size:3.2vh;line-height:1.5;padding:.2em 0;text-shadow:0 2px 8px #000,0 0 2px #000;font-weight:600}
#kl div.on{color:#fff;font-size:4.6vh;}
</style></head><body><div id="p"></div><div id="k"><div id="kl"></div></div><script>
const post=(o)=>window.ipc.postMessage(JSON.stringify(o));
let yt=null,sc=null,scs={pos:0,dur:0,playing:false},kind="",lines=[],cur=-1,vol=80;
function tick(){
  if(kind==="yt"&&yt&&yt.getPlayerState){post({t:"s",playing:yt.getPlayerState()===1,pos:yt.getCurrentTime()||0,dur:yt.getDuration()||0});}
  else if(kind==="sc"&&sc){sc.getPosition(p=>scs.pos=p/1000);sc.getDuration(d=>scs.dur=d/1000);post({t:"s",playing:scs.playing,pos:scs.pos,dur:scs.dur});}
  karaoke();
}
setInterval(tick,250);
function pos(){return kind==="yt"&&yt&&yt.getCurrentTime?yt.getCurrentTime():scs.pos;}
function karaoke(){
  if(!lines.length)return;
  const p=pos();let c=-1;for(let i=0;i<lines.length;i++){if(lines[i][0]<=p)c=i;}
  if(c===cur)return;cur=c;
  const box=document.getElementById("kl");[...box.children].forEach((d,i)=>d.className=i===c?"on":"");
  const el=box.children[Math.max(0,c)];if(el)box.style.transform="translateY(-"+(el.offsetTop+el.offsetHeight/2)+"px)";
}
window.setLyrics=(l,on)=>{lines=l||[];cur=-2;const box=document.getElementById("kl");box.innerHTML="";
  for(const [t,s] of lines){const d=document.createElement("div");d.textContent=s||"♪";box.appendChild(d);}
  document.getElementById("k").style.display=on&&lines.length?"block":"none";box.style.transform="translateY(0)";karaoke();};
window.loadYt=(id,start,auto,v)=>{kind="yt";vol=v;document.getElementById("p").innerHTML='<div id="y"></div>';
  const mk=()=>{yt=new YT.Player("y",{width:"100%",height:"100%",videoId:id,playerVars:{autoplay:auto?1:0,start:Math.floor(start||0),playsinline:1,rel:0,fs:0,iv_load_policy:3,origin:location.origin},
    events:{onReady:e=>{e.target.setVolume(vol);if(auto)e.target.playVideo();post({t:"ready"});},onStateChange:e=>{if(e.data===0)post({t:"end"});},onError:e=>post({t:"err",code:e.data})}});};
  if(window.YT&&YT.Player)mk();else{window.onYouTubeIframeAPIReady=mk;const s=document.createElement("script");s.src="https://www.youtube.com/iframe_api";document.head.appendChild(s);}};
window.loadSc=(url,v)=>{kind="sc";vol=v;scs={pos:0,dur:0,playing:false};
  document.getElementById("p").innerHTML='<iframe id="s" allow="autoplay" src="https://w.soundcloud.com/player/?url='+encodeURIComponent(url)+'&auto_play=true&visual=true"></iframe>';
  const go=()=>{sc=SC.Widget(document.getElementById("s"));const E=SC.Widget.Events;sc.bind(E.READY,()=>{sc.setVolume(vol);post({t:"ready"});});
    sc.bind(E.FINISH,()=>post({t:"end"}));sc.bind(E.PLAY,()=>scs.playing=true);sc.bind(E.PAUSE,()=>scs.playing=false);sc.bind(E.ERROR,()=>post({t:"err",code:0}));};
  if(window.SC)go();else{const s=document.createElement("script");s.src="https://w.soundcloud.com/player/api.js";s.onload=go;document.head.appendChild(s);}};
window.cmd=(c,a)=>{try{
  if(kind==="yt"&&yt){if(c==="play")yt.playVideo();if(c==="pause")yt.pauseVideo();if(c==="seek")yt.seekTo(a,true);if(c==="vol")yt.setVolume(a);if(c==="rate")yt.setPlaybackRate(a);}
  if(kind==="sc"&&sc){if(c==="play")sc.play();if(c==="pause")sc.pause();if(c==="seek")sc.seekTo(a*1000);if(c==="vol")sc.setVolume(a);}
}catch(e){}};
post({t:"boot"});
</script></body></html>"#;

impl Embed {
    pub fn new<W: raw_window_handle::HasWindowHandle>(parent: &W, data_dir: std::path::PathBuf, rect: [f32; 4]) -> Result<Embed, String> {
        let st = Arc::new(Mutex::new(EmbedState::default()));
        let st2 = st.clone();
        let booted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let b2 = booted.clone();
        let mut ctx = WebContext::new(Some(data_dir));
        let view = WebViewBuilder::new_with_web_context(&mut ctx)
            .with_custom_protocol("silo".into(), |_id, _req: Request<Vec<u8>>| {
                Response::builder().header("Content-Type", "text/html; charset=utf-8").body(Cow::Borrowed(PAGE.as_bytes())).unwrap()
            })
            .with_url("http://silo.localhost/player.html")
            .with_autoplay(true)
            .with_background_color((0, 0, 0, 255))
            .with_bounds(rect_of(rect))
            .with_ipc_handler(move |req: Request<String>| {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(req.body()) else { return };
                let mut s = st2.lock().unwrap();
                match v["t"].as_str().unwrap_or("") {
                    "s" => { s.playing = v["playing"].as_bool().unwrap_or(false); s.pos = v["pos"].as_f64().unwrap_or(0.0); s.dur = v["dur"].as_f64().unwrap_or(0.0); }
                    "ready" => s.ready = true,
                    "end" => s.ended = true,
                    "err" => s.error = Some(format!("код {}", v["code"])),
                    "boot" => { s.ready = false; b2.store(true, std::sync::atomic::Ordering::SeqCst); }
                    _ => {}
                }
            })
            .build_as_child(parent)
            .map_err(|e| e.to_string())?;
        Ok(Embed { view, queue: vec![], booted, _ctx: ctx, st, key: String::new() })
    }
    fn js(&mut self, s: &str) {
        if self.booted.load(std::sync::atomic::Ordering::SeqCst) { self.pump(); let _ = self.view.evaluate_script(s); } else { self.queue.push(s.to_string()); }
    }
    /// Отправить отложенные скрипты, когда страница готова (вызывается каждый кадр).
    pub fn pump(&mut self) {
        if self.queue.is_empty() || !self.booted.load(std::sync::atomic::Ordering::SeqCst) { return; }
        for s in std::mem::take(&mut self.queue) { let _ = self.view.evaluate_script(&s); }
    }
    pub fn load_youtube(&mut self, vid: &str, start: f64, autoplay: bool, vol: f32) {
        *self.st.lock().unwrap() = EmbedState::default();
        self.key = format!("youtube:{vid}");
        let js = format!("(function w(){{if(window.loadYt)loadYt({},{},{},{});else setTimeout(w,50);}})()", serde_json::to_string(vid).unwrap(), start, autoplay, (vol * 100.0) as i32);
        self.js(&js);
    }
    pub fn load_soundcloud(&mut self, url: &str, vol: f32) {
        *self.st.lock().unwrap() = EmbedState::default();
        self.key = format!("sc:{url}");
        let js = format!("(function w(){{if(window.loadSc)loadSc({},{});else setTimeout(w,50);}})()", serde_json::to_string(url).unwrap(), (vol * 100.0) as i32);
        self.js(&js);
    }
    pub fn play(&mut self) { self.js("cmd('play')"); }
    pub fn pause(&mut self) { self.js("cmd('pause')"); }
    pub fn seek(&mut self, sec: f64) { self.js(&format!("cmd('seek',{sec})")); }
    pub fn volume(&mut self, v: f32) { self.js(&format!("cmd('vol',{})", (v * 100.0) as i32)); }
    pub fn rate(&mut self, r: f32) { self.js(&format!("cmd('rate',{r})")); }
    /// Строки караоке: [(секунда, текст)]; on — показывать ли их поверх клипа.
    pub fn lyrics(&mut self, lines: &[(f64, String)], on: bool) {
        let arr: Vec<(f64, &str)> = lines.iter().map(|(a, b)| (*a, b.as_str())).collect();
        self.js(&format!("(function w(){{if(window.setLyrics)setLyrics({},{});else setTimeout(w,50);}})()", serde_json::to_string(&arr).unwrap(), on));
    }
    pub fn set_rect(&self, r: [f32; 4]) { let _ = self.view.set_bounds(rect_of(r)); }
    pub fn set_visible(&self, v: bool) { let _ = self.view.set_visible(v); }
}

fn rect_of(r: [f32; 4]) -> Rect {
    Rect { position: LogicalPosition::new(r[0] as f64, r[1] as f64).into(), size: LogicalSize::new(r[2].max(1.0) as f64, r[3].max(1.0) as f64).into() }
}
