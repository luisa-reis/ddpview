//! Answering as a WLED device: the part of WLED's JSON API that programs
//! sending to WLED read before they send, served over HTTP on a thread of
//! its own. What it answers comes from a `Device` the window keeps up to
//! date: the name, the panel's size, and who is sending.
//!
//! It answers as WLED 0.15.0 with one segment and the "Solid" effect.
//! Clients may set `on` and `bri`, which are kept and given back but do
//! not change the picture; every other setting is taken and forgotten.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

/// The port WLED serves its API on, which its clients assume.
pub const HTTP_PORT: u16 = 80;
/// The port used instead when the system refuses one below 1024, as
/// macOS does to programs not run as root unless they listen on every
/// address, and Linux does always.
pub const FALLBACK_PORT: u16 = 8080;
/// The WLED version answered as, and its build number: the API as it is
/// in 0.15.0. Clients check the build for what they can use (DDP needs
/// 2110060 or later).
pub const VERSION: &str = "0.15.0";
pub const BUILD: u32 = 2412100;
/// WLED's own sync port, reported but not listened on.
const SYNC_PORT: u16 = 21324;
/// More than any request a client sends: its first line and headers, and
/// its body.
const HEAD_LIMIT: u64 = 16 * 1024;
const BODY_LIMIT: u64 = 64 * 1024;
/// How long a client may take to send its request or read the answer.
const PATIENCE: Duration = Duration::from_secs(2);
/// How long the server waits between looking for a connection.
const POLL: Duration = Duration::from_millis(20);
/// WLED's error for a body that is not JSON.
const ERROR_JSON: u32 = 9;

/// A made-up network card address: a client keys a device by it, and it
/// ends the device's host name. Locally administered, so it is no real
/// card's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mac(pub [u8; 6]);

impl Mac {
    /// A new one, different each time it is asked for.
    pub fn random() -> Self {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        hasher.write_u128(now.unwrap_or_default().as_nanos());
        hasher.write_u32(std::process::id());
        let bytes = hasher.finish().to_le_bytes();
        let mut mac = [0; 6];
        mac.copy_from_slice(&bytes[..6]);
        // Unicast, and locally administered.
        mac[0] = (mac[0] & 0xfc) | 0x02;
        Self(mac)
    }

    /// Twelve hex digits, as WLED writes it.
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != 12 || !text.is_ascii() {
            return None;
        }
        let mut mac = [0; 6];
        for (i, byte) in mac.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(Self(mac))
    }

    /// The host name a device announces itself by: as WLED's, the last
    /// half of the address after the program's name.
    pub fn host(&self) -> String {
        format!("ddpview-{}", &self.to_string()[6..])
    }
}

impl std::fmt::Display for Mac {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

/// What the device answers with. The window sets the first part; clients
/// set `on` and `bri`; the server counts.
#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub mac: Mac,
    pub width: u32,
    pub height: u32,
    /// Where frames are coming from, while they are.
    pub live: Option<IpAddr>,
    /// The address served on, when it is one address.
    pub ip: Option<IpAddr>,
    pub started: Instant,
    /// As a client last set them.
    pub on: bool,
    pub bri: u8,
    /// Requests answered, and where the last came from.
    pub requests: u64,
    pub asked_by: Option<IpAddr>,
}

impl Device {
    pub fn new(name: &str, mac: Mac, width: u32, height: u32) -> Self {
        Self {
            name: name.into(),
            mac,
            width,
            height,
            live: None,
            ip: None,
            started: Instant::now(),
            on: true,
            bri: 255,
            requests: 0,
            asked_by: None,
        }
    }

    fn count(&self) -> u32 {
        self.width * self.height
    }

    /// A strip is one row; anything taller is a matrix.
    fn matrix(&self) -> bool {
        self.height > 1
    }

    /// `/json/info`.
    pub fn info(&self) -> Value {
        let mut leds = json!({
            "count": self.count(),
            "pwr": 0,
            "fps": 0,
            "maxpwr": 0,
            "maxseg": 1,
            "seglc": [1],
            "lc": 1,
            "rgbw": false,
            "wv": 0,
            "cct": 0,
        });
        if self.matrix() {
            leds["matrix"] = json!({"w": self.width, "h": self.height});
        }
        json!({
            "ver": VERSION,
            "vid": BUILD,
            "cn": "ddpview",
            "release": "ddpview",
            "leds": leds,
            "str": false,
            "name": self.name,
            "udpport": SYNC_PORT,
            "simplifiedui": false,
            "live": self.live.is_some(),
            "liveseg": -1,
            "lm": if self.live.is_some() { "DDP" } else { "" },
            "lip": self.live.map(|ip| ip.to_string()).unwrap_or_default(),
            // No websocket: -1 is how WLED says so.
            "ws": -1,
            "fxcount": 1,
            "palcount": 1,
            "cpalcount": 0,
            "maps": [{"id": 0}],
            "wifi": {"bssid": "", "rssi": 0, "signal": 100, "channel": 0, "ap": false},
            "fs": {"u": 0, "t": 0, "pmt": 0},
            "ndc": 0,
            "arch": "ddpview",
            "core": env!("CARGO_PKG_VERSION"),
            "freeheap": 0,
            "uptime": self.started.elapsed().as_secs(),
            "time": "",
            "opt": 0,
            "brand": "WLED",
            "product": "ddpview",
            "mac": self.mac.to_string(),
            "ip": self.ip.map(|ip| ip.to_string()).unwrap_or_default(),
        })
    }

    /// `/json/state`: the whole panel as one segment.
    pub fn state(&self) -> Value {
        let mut segment = json!({
            "id": 0,
            "start": 0,
            "stop": self.width,
            "len": self.width,
            "grp": 1,
            "spc": 0,
            "of": 0,
            "on": true,
            "frz": false,
            "bri": 255,
            "cct": 127,
            "set": 0,
            "n": "",
            "col": [[255, 160, 0], [0, 0, 0], [0, 0, 0]],
            "fx": 0,
            "sx": 128,
            "ix": 128,
            "pal": 0,
            "c1": 128,
            "c2": 128,
            "c3": 16,
            "sel": true,
            "rev": false,
            "mi": false,
            "o1": false,
            "o2": false,
            "o3": false,
            "si": 0,
            "m12": 0,
        });
        if self.matrix() {
            segment["startY"] = json!(0);
            segment["stopY"] = json!(self.height);
            segment["rY"] = json!(false);
            segment["mY"] = json!(false);
            segment["tp"] = json!(false);
        }
        json!({
            "on": self.on,
            "bri": self.bri,
            "transition": 7,
            "ps": -1,
            "pl": -1,
            "lor": 0,
            "mainseg": 0,
            "nl": {"on": false, "dur": 60, "mode": 1, "tbri": 0, "rem": -1},
            "udpn": {"send": false, "recv": true, "sgrp": 1, "rgrp": 1},
            "seg": [segment],
        })
    }

    /// `/json/cfg`, the settings: enough for a client to see the LEDs and
    /// that realtime data is taken.
    pub fn cfg(&self) -> Value {
        json!({
            "rev": [1, 0],
            "vid": BUILD,
            "id": {"mdns": self.mac.host(), "name": self.name, "inv": "Light", "sui": false},
            "hw": {"led": {
                "total": self.count(),
                "maxpwr": 0,
                "ledma": 0,
                "cct": false,
                "cr": false,
                "cb": 0,
                "fps": 42,
                "rgbwm": 255,
                "ld": false,
                "matrix": {"mpc": 1, "panels": [
                    {"b": false, "r": false, "v": false, "s": false,
                     "x": 0, "y": 0, "w": self.width, "h": self.height},
                ]},
                "ins": [{"start": 0, "len": self.count(), "pin": [2], "order": 0,
                         "rev": false, "skip": 0, "type": 22, "ref": false,
                         "rgbwm": 0, "freq": 0}],
            }},
            "if": {
                "sync": {"port0": SYNC_PORT, "port1": 65506},
                "live": {
                    "en": true, "mso": false, "port": 5568, "mc": false,
                    "dmx": {"uni": 1, "seqskip": false, "e131prio": 0, "addr": 1,
                            "dss": 0, "mode": 4},
                    "timeout": 25, "maxbri": false, "no-gc": true, "offset": 0,
                },
            },
        })
    }

    /// What a client asked to change, from a body sent to `/json/state`:
    /// `on` (true, false or "t" to toggle) and `bri`. Anything else is
    /// taken and forgotten.
    fn change(&mut self, asked: &Map<String, Value>) {
        match asked.get("on") {
            Some(Value::Bool(on)) => self.on = *on,
            Some(Value::String(t)) if t == "t" => self.on = !self.on,
            _ => {}
        }
        if let Some(bri) = asked.get("bri").and_then(Value::as_u64) {
            self.bri = bri.min(255) as u8;
        }
    }

    /// The answer to one request. `body` is what was sent with it.
    pub fn answer(&mut self, method: &str, path: &str, body: &[u8]) -> Reply {
        // A query string asks nothing of the JSON API.
        let path = path.split('?').next().unwrap_or(path);
        let path = path
            .strip_suffix('/')
            .filter(|p| !p.is_empty())
            .unwrap_or(path);
        match (method, path) {
            ("GET", "/") => Reply::html(PAGE.replace("{name}", &escape(&self.name))),
            ("GET", "/json") => Reply::json(json!({
                "state": self.state(),
                "info": self.info(),
                "effects": ["Solid"],
                "palettes": ["Default"],
            })),
            ("GET", "/json/si") => Reply::json(json!({"state": self.state(), "info": self.info()})),
            ("GET", "/json/state") => Reply::json(self.state()),
            ("GET", "/json/info") => Reply::json(self.info()),
            ("GET", "/json/cfg") => Reply::json(self.cfg()),
            ("GET", "/json/eff") => Reply::json(json!(["Solid"])),
            ("GET", "/json/pal") => Reply::json(json!(["Default"])),
            ("GET", "/json/fxdata") => Reply::json(json!([""])),
            ("GET", "/json/nodes") => Reply::json(json!({"nodes": []})),
            ("GET", "/presets.json") => Reply::json(json!({"0": {}})),
            ("POST", "/json" | "/json/state" | "/json/si" | "/json/cfg") => {
                let Ok(Value::Object(asked)) = serde_json::from_slice::<Value>(body) else {
                    return Reply::json(json!({"error": ERROR_JSON})).status(400);
                };
                if path != "/json/cfg" {
                    self.change(&asked);
                }
                // `"v": true` asks for the state back, as it is now.
                if asked.get("v") == Some(&Value::Bool(true)) && path != "/json/cfg" {
                    if path == "/json/si" {
                        return Reply::json(json!({"state": self.state(), "info": self.info()}));
                    }
                    return Reply::json(self.state());
                }
                Reply::json(json!({"success": true}))
            }
            _ => Reply::text("Not Found").status(404),
        }
    }
}

/// The page at `/`, for whoever opens the device in a browser or an app:
/// WLED's interface is not here, only its API.
const PAGE: &str = "<!doctype html><meta charset=utf-8>\
<meta name=viewport content=\"width=device-width\">\
<title>{name}</title>\
<body style=\"font-family:sans-serif;background:#18181b;color:#eee;padding:1em\">\
<h1>{name}</h1>\
<p>This is ddpview, a DDP viewer on a computer. It answers WLED's JSON API \
(<a style=color:#fb3 href=/json/info>/json/info</a>) so that programs that \
send to WLED find it, and shows what they send in its window. It has no \
WLED interface: send it DDP, on UDP port 4048.";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Reply {
    fn json(value: Value) -> Self {
        Self {
            status: 200,
            content_type: "application/json; charset=utf-8",
            body: value.to_string(),
        }
    }

    fn html(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body,
        }
    }

    fn text(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "text/plain; charset=utf-8",
            body: body.into(),
        }
    }

    fn status(self, status: u16) -> Self {
        Self { status, ..self }
    }
}

/// The HTTP server and its thread. Dropping it stops the thread and frees
/// the port.
pub struct Server {
    local: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Binds `addr` and starts answering from `device`.
    pub fn start(addr: SocketAddr, device: Arc<Mutex<Device>>) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let local = listener.local_addr()?;
        // Not waiting on a connection, so that it can look whether to stop.
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("wled".into())
                .spawn(move || serve(&listener, &device, &stop))?
        };
        Ok(Self {
            local,
            stop,
            thread: Some(thread),
        })
    }

    pub fn local(&self) -> SocketAddr {
        self.local
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A panic elsewhere while holding the lock leaves the device as good as
/// it was; carry on with it.
pub fn lock(device: &Mutex<Device>) -> MutexGuard<'_, Device> {
    device
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One connection at a time, one request each: they are few and small.
fn serve(listener: &TcpListener, device: &Mutex<Device>, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            // A client that goes wrong is that client's problem.
            Ok((stream, from)) => {
                let _ = respond(stream, from, device);
            }
            Err(_) => thread::sleep(POLL),
        }
    }
}

/// Reads one HTTP request and answers it, closing the connection after.
fn respond(stream: TcpStream, from: SocketAddr, device: &Mutex<Device>) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(PATIENCE))?;
    stream.set_write_timeout(Some(PATIENCE))?;
    let mut reader = BufReader::new(&stream).take(HEAD_LIMIT);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut words = line.split_whitespace();
    let (Some(method), Some(path)) = (words.next(), words.next()) else {
        return Err(io::ErrorKind::InvalidData.into());
    };
    let (method, path) = (method.to_string(), path.to_string());
    let mut length = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = Vec::new();
    let mut reader = reader.into_inner();
    reader
        .by_ref()
        .take(length.min(BODY_LIMIT))
        .read_to_end(&mut body)?;
    let head = method == "HEAD";
    let method = match method.as_str() {
        "GET" | "HEAD" => "GET",
        "POST" => "POST",
        _ => "OTHER",
    };
    let reply = {
        let mut device = lock(device);
        device.requests += 1;
        device.asked_by = Some(from.ip());
        device.answer(method, &path, &body)
    };
    let mut out = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        reply.status,
        reason(reply.status),
        reply.content_type,
        reply.body.len(),
    );
    if !head {
        out.push_str(&reply.body);
    }
    (&stream).write_all(out.as_bytes())?;
    (&stream).flush()
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Device {
        Device::new("Kitchen", Mac([0x02, 0x11, 0x22, 0xaa, 0xbb, 0xcc]), 64, 32)
    }

    fn get(device: &mut Device, path: &str) -> Value {
        let reply = device.answer("GET", path, b"");
        assert_eq!(reply.status, 200, "{path}");
        serde_json::from_str(&reply.body).expect("JSON")
    }

    #[test]
    fn a_mac_is_written_and_read_as_twelve_hex_digits() {
        let mac = Mac([0x02, 0x11, 0x22, 0xaa, 0xbb, 0xcc]);
        assert_eq!(mac.to_string(), "021122aabbcc");
        assert_eq!(Mac::parse("021122aabbcc"), Some(mac));
        assert_eq!(Mac::parse("021122AABBCC"), Some(mac));
        for bad in [
            "",
            "021122aabbc",
            "021122aabbccdd",
            "02112zaabbcc",
            "02:11:22:aa",
        ] {
            assert_eq!(Mac::parse(bad), None, "{bad:?}");
        }
        assert_eq!(mac.host(), "ddpview-aabbcc");
        let random = Mac::random();
        assert_eq!(random.0[0] & 0x03, 0x02, "locally administered unicast");
        assert_ne!(random, Mac::random());
    }

    #[test]
    fn info_says_wled_with_the_panel_and_the_sender() {
        let mut d = device();
        let info = get(&mut d, "/json/info");
        assert_eq!(info["brand"], "WLED", "clients check for the brand");
        assert!(
            info["vid"].as_u64().unwrap() >= 2110060,
            "new enough for DDP"
        );
        assert_eq!(info["name"], "Kitchen");
        assert_eq!(info["mac"], "021122aabbcc");
        assert_eq!(info["leds"]["count"], 2048);
        assert_eq!(info["leds"]["matrix"], json!({"w": 64, "h": 32}));
        assert_eq!(info["leds"]["rgbw"], false);
        assert_eq!(
            (info["live"].clone(), info["lm"].clone()),
            (json!(false), json!(""))
        );

        d.live = Some("192.168.1.20".parse().unwrap());
        let info = get(&mut d, "/json/info");
        assert_eq!(info["live"], true);
        assert_eq!(info["lm"], "DDP");
        assert_eq!(info["lip"], "192.168.1.20");
    }

    #[test]
    fn a_single_row_is_a_strip_not_a_matrix() {
        let mut d = Device::new("Strip", Mac([2, 0, 0, 0, 0, 1]), 300, 1);
        let info = get(&mut d, "/json/info");
        assert_eq!(info["leds"]["count"], 300);
        assert!(info["leds"].get("matrix").is_none());
        let state = get(&mut d, "/json/state");
        assert!(state["seg"][0].get("stopY").is_none());
        assert_eq!(state["seg"][0]["stop"], 300);
    }

    #[test]
    fn the_state_is_one_segment_over_the_whole_panel() {
        let mut d = device();
        let state = get(&mut d, "/json/state/");
        let seg = &state["seg"][0];
        assert_eq!(
            (seg["start"].clone(), seg["stop"].clone()),
            (json!(0), json!(64))
        );
        assert_eq!(
            (seg["startY"].clone(), seg["stopY"].clone()),
            (json!(0), json!(32))
        );
        assert_eq!(state["seg"].as_array().unwrap().len(), 1);
        let all = get(&mut d, "/json");
        assert_eq!(all["state"], state);
        assert_eq!(all["effects"], json!(["Solid"]));
        let si = get(&mut d, "/json/si?x=1");
        assert_eq!(si["info"]["name"], "Kitchen");
        let cfg = get(&mut d, "/json/cfg");
        assert_eq!(cfg["hw"]["led"]["total"], 2048);
        assert_eq!(cfg["if"]["live"]["en"], true);
    }

    #[test]
    fn clients_may_turn_it_off_and_set_the_brightness() {
        let mut d = device();
        let reply = d.answer("POST", "/json/state", br#"{"on": false, "bri": 128}"#);
        assert_eq!(reply.body, r#"{"success":true}"#);
        assert!(!d.on);
        assert_eq!(d.bri, 128);
        d.answer("POST", "/json/state", br#"{"on": "t", "bri": 999}"#);
        assert!(d.on, "toggled");
        assert_eq!(d.bri, 255, "held to a byte");
        let reply = d.answer("POST", "/json", br#"{"bri": 10, "v": true}"#);
        let state: Value = serde_json::from_str(&reply.body).unwrap();
        assert_eq!(state["bri"], 10, "the state given back");
        d.answer("POST", "/json/cfg", br#"{"on": false, "if": {}}"#);
        assert!(d.on, "settings are not the state");
    }

    #[test]
    fn what_is_not_understood_is_said_so() {
        let mut d = device();
        let reply = d.answer("POST", "/json/state", b"{not json");
        assert_eq!((reply.status, reply.body.as_str()), (400, r#"{"error":9}"#));
        assert_eq!(d.answer("POST", "/json/state", b"[1]").status, 400);
        assert_eq!(d.answer("GET", "/win", b"").status, 404);
        assert_eq!(d.answer("OTHER", "/json", b"").status, 404);
        let page = d.answer("GET", "/", b"");
        assert!(page.content_type.starts_with("text/html"));
        assert!(page.body.contains("<h1>Kitchen</h1>"));
        d.name = "<b>".into();
        assert!(d.answer("GET", "/", b"").body.contains("&lt;b&gt;"));
    }

    #[test]
    fn the_server_answers_over_http_and_counts() {
        let device = Arc::new(Mutex::new(device()));
        let server = Server::start("127.0.0.1:0".parse().unwrap(), Arc::clone(&device))
            .expect("a free port");
        let ask = |request: &str| {
            let mut stream = TcpStream::connect(server.local()).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        let reply = ask("GET /json/info HTTP/1.0\r\n\r\n");
        assert!(reply.starts_with("HTTP/1.1 200 OK"), "{reply}");
        assert!(reply.contains("application/json"), "{reply}");
        assert!(reply.contains(r#""brand":"WLED""#), "{reply}");
        let body = r#"{"bri":42}"#;
        let reply = ask(&format!(
            "POST /json/state HTTP/1.0\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        assert!(reply.ends_with(r#"{"success":true}"#), "{reply}");
        let d = lock(&device);
        assert_eq!(d.bri, 42);
        assert_eq!(d.requests, 2);
        assert_eq!(d.asked_by, Some("127.0.0.1".parse().unwrap()));
        drop(d);
        let local = server.local();
        drop(server);
        assert!(
            TcpListener::bind(local).is_ok(),
            "the port is free once the server is dropped"
        );
    }
}
