//! Polite networking: when a site fails (no answer, too many requests, or a
//! server error), Cerberus leaves it alone for a while before asking again:
//! 30 s, then twice that each time, up to an hour, or what the site asks for
//! in `Retry-After`. The first good answer clears it.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cerberus_shared::tr;
use reqwest::{RequestBuilder, Response, StatusCode};

const FIRST: Duration = Duration::from_secs(30);
const MOST: Duration = Duration::from_secs(3600);

/// Reads an answer's body up to `max` bytes, as it arrives: a server that
/// says nothing about the size (or lies) and never stops can't fill the memory.
pub async fn read_limited(reply: Response, max: u64) -> Result<Vec<u8>, String> {
    use futures_util::StreamExt;
    if reply.content_length().is_some_and(|n| n > max) {
        return Err(tr!("A resposta é grande demais.").into());
    }
    let mut body = Vec::new();
    let mut stream = reply.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.without_url().to_string())?;
        if body.len() as u64 + chunk.len() as u64 > max {
            return Err(tr!("A resposta é grande demais.").into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Follows redirects (up to 10) but never from https to plain http: a
/// download (a recomp, a core) never comes in without TLS halfway.
pub fn redirects() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let leaving_tls = attempt.previous().iter().any(|u| u.scheme() == "https") && attempt.url().scheme() != "https";
        if leaving_tls {
            attempt.error("redirected from https to http")
        } else if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else {
            attempt.follow()
        }
    })
}

/// Why a request didn't get an answer.
#[derive(Debug)]
pub enum Failed {
    /// The site failed lately; it's left alone for this long still.
    Resting(Duration),
    Network(reqwest::Error),
}

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failed::Resting(left) => write!(f, "{}", tr!("O site falhou há pouco; o Cerberus tenta de novo em {} s.", left.as_secs().max(1))),
            Failed::Network(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Default)]
struct Hosts(HashMap<String, (Instant, Duration)>);

impl Hosts {
    /// How long the host is still left alone, if at all.
    fn resting(&self, host: &str, now: Instant) -> Option<Duration> {
        let (until, _) = self.0.get(host)?;
        until.checked_duration_since(now).filter(|d| !d.is_zero())
    }

    /// A failure: the wait doubles (or is what the site asked for).
    fn failed(&mut self, host: &str, now: Instant, asked: Option<Duration>) {
        let last = self.0.get(host).map(|(_, wait)| *wait);
        let wait = asked.unwrap_or_else(|| last.map_or(FIRST, |w| w * 2)).clamp(FIRST, MOST);
        self.0.insert(host.to_string(), (now + wait, wait));
    }

    fn answered(&mut self, host: &str) {
        self.0.remove(host);
    }
}

static HOSTS: Mutex<Option<Hosts>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Hosts) -> T) -> T {
    let mut hosts = HOSTS.lock().unwrap_or_else(|p| p.into_inner());
    f(hosts.get_or_insert_with(Hosts::default))
}

/// Answers that mean "not now": too many requests, or the server is in trouble.
fn overloaded(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// `Retry-After` in seconds (the date form is rare and ignored).
fn retry_after(reply: &Response) -> Option<Duration> {
    let secs: u64 = reply.headers().get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()?;
    Some(Duration::from_secs(secs))
}

/// GitHub's own limit: 403 with no requests left, and when they come back (Unix seconds).
fn github_limit(reply: &Response) -> Option<Option<Duration>> {
    let header = |name: &str| reply.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    if reply.status() != StatusCode::FORBIDDEN || header("x-ratelimit-remaining")? != "0" {
        return None;
    }
    let reset: u64 = header("x-ratelimit-reset").and_then(|v| v.parse().ok()).unwrap_or(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    Some((reset > now).then(|| Duration::from_secs(reset - now)))
}

/// Sends the request unless its site is resting, and notes how the site answered.
pub async fn send(request: RequestBuilder) -> Result<Response, Failed> {
    let (client, built) = request.build_split();
    // Errors never carry the address: it can hold a key or a token (`?key=`, `access_token=`).
    let request = built.map_err(|e| Failed::Network(e.without_url()))?;
    let host = request.url().host_str().unwrap_or_default().to_string();
    if let Some(left) = with(|h| h.resting(&host, Instant::now())) {
        return Err(Failed::Resting(left));
    }
    match client.execute(request).await {
        Ok(reply) if overloaded(reply.status()) => {
            let asked = retry_after(&reply);
            with(|h| h.failed(&host, Instant::now(), asked));
            Ok(reply)
        }
        Ok(reply) if github_limit(&reply).is_some() => {
            let asked = github_limit(&reply).flatten();
            with(|h| h.failed(&host, Instant::now(), asked));
            Ok(reply)
        }
        Ok(reply) => {
            with(|h| h.answered(&host));
            Ok(reply)
        }
        Err(e) => {
            // A request that couldn't even be sent says nothing about the site.
            if e.is_timeout() || e.is_connect() {
                with(|h| h.failed(&host, Instant::now(), None));
            }
            Err(Failed::Network(e.without_url()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serve(reply: String) -> String {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                let _: Vec<_> = BufReader::new(&socket).lines().map_while(Result::ok).take_while(|l| !l.is_empty()).collect();
                let _ = socket.write_all(reply.as_bytes());
            }
        });
        url
    }

    #[test]
    fn an_answer_without_a_size_stops_at_the_limit() {
        // Chunked: no Content-Length to check first.
        let url = serve("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\n0123456789abcdef\r\n10\r\n0123456789abcdef\r\n0\r\n\r\n".into());
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let reply = tauri::async_runtime::block_on(client.get(&url).send()).unwrap();
        assert!(tauri::async_runtime::block_on(read_limited(reply, 20)).is_err());
        let url = serve("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".into());
        let reply = tauri::async_runtime::block_on(client.get(&url).send()).unwrap();
        assert_eq!(tauri::async_runtime::block_on(read_limited(reply, 20)).unwrap(), b"ok");
    }

    #[test]
    fn redirects_are_still_followed_without_leaving_tls() {
        let client = reqwest::Client::builder().no_proxy().redirect(redirects()).build().unwrap();
        let target = serve("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".into());
        let first = serve(format!("HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\n\r\n"));
        let reply = tauri::async_runtime::block_on(client.get(&first).send()).unwrap();
        assert_eq!(reply.status(), 200);
    }

    #[test]
    fn a_failed_request_never_shows_its_address() {
        let client = reqwest::Client::new();
        // Nothing listens on port 9 of this machine: the request fails at once.
        let failed = tauri::async_runtime::block_on(send(client.get("http://127.0.0.1:9/api?key=SECRET&access_token=TOKEN")));
        let text = match failed {
            Err(e) => e.to_string(),
            Ok(_) => String::new(),
        };
        assert!(!text.contains("SECRET") && !text.contains("TOKEN"), "{text}");
    }

    #[test]
    fn a_failing_site_rests_longer_each_time_and_is_cleared_by_an_answer() {
        let mut hosts = Hosts::default();
        let t = Instant::now();
        assert_eq!(hosts.resting("api.github.com", t), None);
        hosts.failed("api.github.com", t, None);
        assert_eq!(hosts.resting("api.github.com", t), Some(FIRST));
        assert_eq!(hosts.resting("example.com", t), None, "other sites are unaffected");
        hosts.failed("api.github.com", t, None);
        assert_eq!(hosts.resting("api.github.com", t), Some(FIRST * 2));
        for _ in 0..20 {
            hosts.failed("api.github.com", t, None);
        }
        assert_eq!(hosts.resting("api.github.com", t), Some(MOST), "never more than an hour");
        assert_eq!(hosts.resting("api.github.com", t + MOST), None, "and then it may be asked again");
        hosts.answered("api.github.com");
        hosts.failed("api.github.com", t, None);
        assert_eq!(hosts.resting("api.github.com", t), Some(FIRST), "a good answer starts over");
    }

    #[test]
    fn the_site_can_say_how_long() {
        let mut hosts = Hosts::default();
        let t = Instant::now();
        hosts.failed("h", t, Some(Duration::from_secs(120)));
        assert_eq!(hosts.resting("h", t), Some(Duration::from_secs(120)));
        hosts.failed("h", t, Some(Duration::from_secs(5)));
        assert_eq!(hosts.resting("h", t), Some(FIRST), "at least the first wait");
        assert!(overloaded(StatusCode::TOO_MANY_REQUESTS) && overloaded(StatusCode::BAD_GATEWAY));
        assert!(!overloaded(StatusCode::NOT_FOUND) && !overloaded(StatusCode::OK));
    }
}
