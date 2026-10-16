//! Stateless Cloudflare Worker & Hibernating Durable Object signaling bootstrap crate.

pub use zero_protocol;

pub mod durable_object;
pub mod handlers;

pub use durable_object::{
    DeadDropRoom, EphemeralRoomState, HibernatingSocket, HostAttachment, MockHostSocket,
    DEAD_DROP_BINDING, HOST_WEBSOCKET_TAG, MAX_BURNED_UUIDS,
};
pub use handlers::{
    extract_bearer_token, parse_host_secret, process_dead_drop, process_host_upgrade, HttpResponse,
    NetworkError, MAX_DROP_BODY_BYTES,
};

use worker::{event, Context, Env, Request, Response, Result, Router};

use crate::handlers::{cors_preflight_response, with_cors};

/// Cloudflare Worker fetch entrypoint routing `/host/:do_id` and `/drop/:do_id/:invite_uuid`
/// to the named `DeadDropRoom` Durable Object instance.
#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();

    router
        .options("/host/:do_id", |_, _| cors_preflight_response())
        .options("/drop/:do_id/:invite_uuid", |_, _| {
            cors_preflight_response()
        })
        .get_async("/host/:do_id", |req, ctx| async move {
            let do_id = match ctx.param("do_id") {
                Some(id) if !id.trim().is_empty() => id,
                _ => return Response::error("Missing Durable Object ID", 400),
            };
            let namespace = ctx.durable_object(DEAD_DROP_BINDING)?;
            let stub = namespace.id_from_name(do_id)?.get_stub()?;
            stub.fetch_with_request(req).await
        })
        .post_async("/drop/:do_id/:invite_uuid", |req, ctx| async move {
            let do_id = match ctx.param("do_id") {
                Some(id) if !id.trim().is_empty() => id,
                _ => return with_cors(Response::error("Missing Durable Object ID", 400)?),
            };
            let namespace = ctx.durable_object(DEAD_DROP_BINDING)?;
            let stub = namespace.id_from_name(do_id)?.get_stub()?;
            let resp = stub.fetch_with_request(req).await?;
            with_cors(resp)
        })
        .get("/health", |_, _| with_cors(Response::ok("ok")?))
        .run(req, env)
        .await
}
