//! A scripted HTTP service for tests of the storage clients. It listens on a loopback port,
//! answers each request with the next reply scripted for it, and records the requests, so a
//! test can have the service fail the ways a busy or broken one does.

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What the service does with a request, once it has read it whole.
#[derive(Debug, Clone)]
pub enum Reply {
    /// An answer with this status, headers and body. An answer to `HEAD` leaves the body out,
    /// and its headers say how long the object is.
    Answer {
        status: u16,
        headers: Vec<(&'static str, String)>,
        body: String,
    },
    /// Closes the connection without answering, as when it drops after the request went out.
    HangUp,
    /// Never answers.
    Silence,
}

impl Reply {
    pub fn status(status: u16, body: &str) -> Self {
        Self::Answer {
            status,
            headers: Vec::new(),
            body: body.to_string(),
        }
    }

    pub fn with_header(mut self, name: &'static str, value: &str) -> Self {
        if let Self::Answer { headers, .. } = &mut self {
            headers.push((name, value.to_string()));
        }
        self
    }
}

/// The replies for the requests whose first line starts with the prefix, such as
/// `POST /bucket/key?uploads=`, in turn. The last one answers every request after it.
pub type Rule = (&'static str, Vec<Reply>);

type Script = Arc<Mutex<Vec<Rule>>>;
type Requests = Arc<Mutex<Vec<String>>>;

pub struct ScriptedService {
    /// `http://127.0.0.1:<port>`.
    pub address: String,
    requests: Requests,
}

impl ScriptedService {
    /// Starts a service that answers by the rules `script` makes from its address. A request
    /// that no rule names gets a 404.
    pub async fn start(script: impl FnOnce(&str) -> Vec<Rule>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let script: Script = Arc::new(Mutex::new(script(&address)));
        let requests: Requests = Arc::default();
        let taken = requests.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(serve(stream, script.clone(), taken.clone()));
            }
        });
        Self { address, requests }
    }

    /// How many of the requests taken so far start with `prefix`.
    pub fn count(&self, prefix: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.starts_with(prefix))
            .count()
    }

    /// Waits up to `limit` for a request that starts with `prefix`.
    pub async fn waits_for(&self, prefix: &str, limit: Duration) -> bool {
        let started = Instant::now();
        while self.count(prefix) == 0 {
            if started.elapsed() > limit {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        true
    }
}

async fn serve(mut stream: TcpStream, script: Script, requests: Requests) {
    let Some(head) = read_request(&mut stream).await else {
        return;
    };
    let line = head.lines().next().unwrap_or_default().to_string();
    let reply = next_reply(&script, &line);
    requests.lock().unwrap().push(line.clone());
    match reply {
        Reply::HangUp => {}
        // Holds the connection until the client gives up on it.
        Reply::Silence => {
            let _ = stream.read(&mut [0u8; 1]).await;
        }
        Reply::Answer {
            status,
            headers,
            body,
        } => {
            let with_body = !line.starts_with("HEAD ");
            let mut answer = format!("HTTP/1.1 {status} Scripted\r\nconnection: close\r\n");
            for (name, value) in headers {
                answer.push_str(&format!("{name}: {value}\r\n"));
            }
            if with_body {
                answer.push_str(&format!("content-length: {}\r\n\r\n{body}", body.len()));
            } else {
                answer.push_str("\r\n");
            }
            let _ = stream.write_all(answer.as_bytes()).await;
        }
    }
}

/// Reads a request with the body its `content-length` gives, and returns its head.
async fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut received = Vec::new();
    let mut piece = vec![0u8; 64 * 1024];
    let head_end = loop {
        let read = stream
            .read(&mut piece)
            .await
            .ok()
            .filter(|read| *read > 0)?;
        received.extend_from_slice(&piece[..read]);
        if let Some(position) = received.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let head = String::from_utf8_lossy(&received[..head_end]).into_owned();
    let length: usize = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body_length = received.len() - head_end;
    while body_length < length {
        let read = stream
            .read(&mut piece)
            .await
            .ok()
            .filter(|read| *read > 0)?;
        body_length += read;
    }
    Some(head)
}

fn next_reply(script: &Mutex<Vec<Rule>>, line: &str) -> Reply {
    let mut script = script.lock().unwrap();
    let Some((_, replies)) = script
        .iter_mut()
        .find(|(prefix, _)| line.starts_with(prefix))
    else {
        return Reply::status(404, "");
    };
    match replies.len() {
        0 => Reply::status(404, ""),
        1 => replies[0].clone(),
        _ => replies.remove(0),
    }
}
