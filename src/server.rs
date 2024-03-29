use std::pin::Pin;
use std::str;
use std::task::{Context, Poll};

use futures::stream::Stream;
use log::info;
use serde::Deserialize;
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use warp::reject::{not_found, Rejection};
use warp::reply::{json, with_header};
use warp::{path::FullPath, Filter, Reply};

use crate::appstate::AppState;
use crate::assets::Webapp;
use crate::events::{Command, Event, MessageReceiver, MessageSender};

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Request {}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum MessageFromClient {
    Command { command: Command },
    Request { id: u32, request: Request },
}

enum MessageToClient {
    Event { event: Event },
    Response { id: u32, response: Value },
}

#[derive(Clone)]
pub struct ClientInfo {
    pub app_state: AppState,
    pub command_sender: MessageSender<Command>,
    pub event_receiver: MessageReceiver<Event>,
}

struct Incoming {
    listener: TcpListener,
}

impl Stream for Incoming {
    type Item = tokio::io::Result<TcpStream>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.listener
            .poll_accept(cx)
            .map(|result| Some(result.map(|(stream, _)| stream)))
    }
}

async fn static_content(path: FullPath) -> Result<impl Reply, Rejection> {
    let mut target = &path.as_str()[1..];
    if target.is_empty() {
        target = "index.html";
    }

    let data = match Webapp::get(target) {
        Some(data) => str::from_utf8(&data).unwrap().to_owned(),
        None => return Err(not_found()),
    };

    let last_part = match target.rfind('/') {
        Some(pos) => &target[pos + 1..],
        None => target,
    };

    let content_type = match last_part.rfind('.') {
        Some(0) => "text/plain",
        Some(pos) => match &last_part[pos + 1..] {
            "html" => "text/html",
            "css" => "text/css",
            "js" => "text/javascript",
            _ => "text/plain",
        },
        None => "text/plain",
    };

    Ok(with_header(data, "content-type", content_type))
}

fn static_content_route() -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path::full().and_then(static_content)
}

async fn state(info: ClientInfo) -> Result<impl Reply, Rejection> {
    Ok(json(&info.app_state))
}

fn state_route(
    info: ClientInfo,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path("state")
        .and(warp::path::end())
        .and_then(move || state(info.clone()))
}

fn api_routes(
    info: &ClientInfo,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path("api").and(state_route(info.clone()))
}

pub fn serve(listener: TcpListener, info: ClientInfo) {
    let server = warp::serve(
        api_routes(&info)
            .or(static_content_route())
            .with(warp::log("musicbox::server")),
    );

    if let Ok(addr) = listener.local_addr() {
        info!("Starting webserver, listening on {}.", addr);
    }

    tokio::spawn(server.serve_incoming(Incoming { listener }));
}
