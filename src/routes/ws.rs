use crate::state::{ws_idle_timeout, AppState};
use actix_web::{web, Error as ActixError, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt;

pub async fn ws(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, ActixError> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    let tracker = state.ws.clone();
    tracker.on_connect();
    actix_web::rt::spawn(async move {
        // The browser pings every 10s; if nothing arrives within the idle
        // timeout the page is considered gone (tab closed / machine asleep).
        loop {
            let next = actix_web::rt::time::timeout(ws_idle_timeout(), msg_stream.next()).await;
            match next {
                Ok(Some(Ok(msg))) => match msg {
                    Message::Ping(bytes) => {
                        let _ = session.pong(&bytes).await;
                    }
                    Message::Text(text) => {
                        if text.trim() == "ping" {
                            let _ = session.text(String::from("pong")).await;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                },
                Ok(Some(Err(_))) | Ok(None) => break,
                Err(_) => break, // idle timeout
            }
        }
        let _ = session.close(None).await;
        tracker.on_disconnect();
    });
    Ok(response)
}
