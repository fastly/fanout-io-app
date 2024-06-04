use fastly::http::StatusCode;
use fastly::{Error, Request, Response};
use std::collections::HashMap;

/// Returns a GRIP response to initialize a stream
///
/// When our app receives a non-WebSocket request (i.e. normal HTTP) and wants
/// to make it long lived (longpoll or SSE), we call handoff_fanout on it, and
/// Fanout will then forward that request to the nominated backend.  In this app,
/// that backend is this same Compute service, where we then need to respond
/// with some Grip headers to tell Fanout to hold the connection for streaming.
/// This function constructs such a response.
pub fn grip_response(ctype: &str, ghold: &str, chan: &str) -> Response {
    Response::from_status(StatusCode::OK)
        .with_header("Content-Type", ctype)
        .with_header("Grip-Hold", ghold)
        .with_header("Grip-Channel", chan)
        .with_body("")
}

/// Returns a WebSocket-over-HTTP formatted TEXT message
pub fn ws_text(msg: &str) -> Vec<u8> {
    format!("TEXT {:02x}\r\n{}\r\n", msg.len(), msg)
        .as_bytes()
        .to_vec()
}

// Returns a channel-subscription command in a WebSocket-over-HTTP format
pub fn ws_sub(ch: &str) -> Vec<u8> {
    ws_text(format!("c:{{\"type\":\"subscribe\",\"channel\":\"{}\"}}", ch).as_str())
}

fn handle_test_ws(mut req: Request, chan: &str) -> Response {
    if req.get_header_str("Content-Type") != Some("application/websocket-events") {
        return Response::from_status(StatusCode::BAD_REQUEST)
            .with_body("Not a WebSocket-over-HTTP request.\n");
    }

    let req_body = req.take_body().into_bytes();
    let mut resp_body: Vec<u8> = [].to_vec();

    let mut resp = Response::from_status(StatusCode::OK)
        .with_header("Content-Type", "application/websocket-events");

    if req_body.starts_with(b"OPEN\r\n") {
        resp.set_header("Sec-WebSocket-Extensions", "grip; message-prefix=\"\"");
        resp_body.extend("OPEN\r\n".as_bytes());
        resp_body.extend(ws_sub(chan));
        resp_body.extend(ws_text(
            "c:{\"type\":\"keep-alive\",\"message-type\":\"ping\",\"content\":\"\",\"timeout\":20}",
        ));
    }

    let close = b"CLOSE".as_slice();
    if req_body.windows(close.len()).any(|w| w == close) {
        resp_body.extend(b"CLOSE\r\n");
    }

    resp.set_body(resp_body);
    resp
}

fn handle_test(req: Request, chan: &str) -> Response {
    match req.get_url().path() {
        "/test" | "/test/" => {
            Response::from_status(StatusCode::OK).with_body("Hello from the Fanout test handler!\n")
        }
        "/test/sse" => {
            let mut padding = b":".to_vec();
            padding.extend(vec![b' '; 2048]);
            padding.extend(b"\n\n");

            grip_response("text/event-stream", "stream", chan)
                .with_header("Grip-Keep-Alive", ":\\n\\n; format=cstring; timeout=20")
                .with_body(padding)
        }
        "/test/ws" => handle_test_ws(req, chan),
        _ => Response::from_status(StatusCode::NOT_FOUND).with_body("{\"error\": \"not found\"}\n"),
    }
}

const EVENTSOURCE_MIN_JS: &str = include_str!("../static/eventsource.min.js");
const FAYE_BROWSER_1_1_2_FANOUT1_MIN_JS: &str =
    include_str!("../static/faye-browser-1.1.2-fanout1-min.js");
const FAYE_BROWSER_1_1_2_FANOUT1_MIN_JS_MAP: &str =
    include_str!("../static/faye-browser-1.1.2-fanout1-min.js.map");
const FAYE_BROWSER_1_1_2_FANOUT1_JS: &str = include_str!("../static/faye-browser-1.1.2-fanout1.js");
const FAYE_BROWSER_MIN_JS: &str = include_str!("../static/faye-browser-min.js");
const FAYE_BROWSER_MIN_JS_MAP: &str = include_str!("../static/faye-browser-min.js.map");
const FAYE_BROWSER_JS: &str = include_str!("../static/faye-browser.js");
const JSON2_JS: &str = include_str!("../static/json2.js");
const RECONNECTING_EVENTSOURCE_JS: &str = include_str!("../static/reconnecting-eventsource.js");

fn handle_static(req: Request) -> Response {
    let fname = req.get_url().path_segments().unwrap().last().unwrap();

    let mut files = HashMap::new();
    files.insert("eventsource.min.js", EVENTSOURCE_MIN_JS);
    files.insert(
        "faye-browser-1.1.2-fanout1-min.js",
        FAYE_BROWSER_1_1_2_FANOUT1_MIN_JS,
    );
    files.insert(
        "faye-browser-1.1.2-fanout1-min.js.map",
        FAYE_BROWSER_1_1_2_FANOUT1_MIN_JS_MAP,
    );
    files.insert(
        "faye-browser-1.1.2-fanout1.js",
        FAYE_BROWSER_1_1_2_FANOUT1_JS,
    );
    files.insert("faye-browser-min.js", FAYE_BROWSER_MIN_JS);
    files.insert("faye-browser-min.js.map", FAYE_BROWSER_MIN_JS_MAP);
    files.insert("faye-browser.js", FAYE_BROWSER_JS);
    files.insert("json2.js", JSON2_JS);
    files.insert("reconnecting-eventsource.js", RECONNECTING_EVENTSOURCE_JS);

    let data = match files.get(fname) {
        Some(s) => s,
        None => return Response::from_status(StatusCode::NOT_FOUND),
    };

    let ctype = if fname.ends_with(".js") {
        "application/javascript"
    } else if fname.ends_with(".map") {
        "application/octet-stream"
    } else {
        "text/plain"
    };

    Response::from_status(StatusCode::OK)
        .with_header("Content-Type", ctype.as_bytes())
        .with_body(data.as_bytes())
}

fn is_tls(req: &Request) -> bool {
    req.get_url().scheme().eq_ignore_ascii_case("https")
}

fn main() -> Result<(), Error> {
    // Log service version
    println!(
        "FASTLY_SERVICE_VERSION: {}",
        std::env::var("FASTLY_SERVICE_VERSION").unwrap_or_else(|_| String::new())
    );

    let mut req = Request::from_client().with_pass(true);

    let host = match req.get_url().host_str() {
        Some(s) => s.to_string(),
        None => {
            Response::from_status(StatusCode::NOT_FOUND)
                .with_body("Unknown host\n")
                .send_to_client();
            return Ok(());
        }
    };

    let path = req.get_path().to_string();

    if let Some(addr) = req.get_client_ip_addr() {
        req.set_header("X-Forwarded-For", addr.to_string());
    }

    if is_tls(&req) {
        req.set_header("X-Forwarded-Proto", "https");
    }

    if host.ends_with(".fanoutcdn.com") {
        if path.starts_with("/test/static/") || path.starts_with("/bayeux/static/") {
            handle_static(req).send_to_client();
            return Ok(());
        }

        if path == "/test" || path.starts_with("/test/") {
            if req.get_header_str("Grip-Sig").is_some() {
                // request is from fanout
                handle_test(req, "test").send_to_client();
            } else {
                // not from fanout, hand it off to fanout to manage
                let backend = format!("self_{}", host);
                println!("handoff to backend {backend}");
                req.handoff_fanout(&backend).map_err(|e| {
                    println!("Some error happened: {e:?}");
                    e
                })?;
            }

            return Ok(());
        }

        if path == "/bayeux" || path.starts_with("/bayeux/") {
            return Ok(req.handoff_fanout("bayeux-handler")?);
        }
    }

    let backend = {
        let backend_prefix = if is_tls(&req) {
            "https_backend_"
        } else {
            "http_backend_"
        };

        format!("{}{}", backend_prefix, host)
    };

    println!("handoff to backend {backend}");
    req.handoff_fanout(backend.as_str()).map_err(|e| {
        println!("Some error happened: {e:?}");
        e
    })?;

    Ok(())
}
