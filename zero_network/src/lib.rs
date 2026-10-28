//! Stateless Cloudflare Worker & Hibernating Durable Object signaling bootstrap crate.

pub use zero_protocol;

pub mod durable_object;
pub mod handlers;

pub use durable_object::{
    DeadDropRoom, EphemeralRoomState, HibernatingSocket, HostAttachment, MockHostSocket,
    DEAD_DROP_BINDING, HOST_WEBSOCKET_TAG, MAX_BURNED_UUIDS,
};
pub use handlers::{
    build_default_stun_response, cors_preflight_response, default_ice_servers_json,
    extract_bearer_token, get_env_var, handle_ice_config, parse_custom_ice_servers,
    parse_host_secret, process_dead_drop, process_host_upgrade, process_ice_servers, with_cors,
    HttpResponse, NetworkError, MAX_DROP_BODY_BYTES,
};

use worker::{event, Context, Env, Request, Response, Result, Router};

/// Cloudflare Worker fetch entrypoint routing `/host/:do_id` and `/drop/:do_id/:invite_uuid`
/// to the named `DeadDropRoom` Durable Object instance, and `/ice` for TURN/STUN negotiation.
#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();

    router
        .options("/host/:do_id", |_, _| cors_preflight_response())
        .options("/drop/:do_id/:invite_uuid", |_, _| {
            cors_preflight_response()
        })
        .options("/ice", |_, _| cors_preflight_response())
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
            let mut resp = match stub.fetch_with_request(req).await {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = Response::error(format!("Durable Object error: {e}"), 500)?;
                    return with_cors(err_resp);
                }
            };

            // Never mutate headers on WebSocket 101 Switching Protocols
            if resp.status_code() == 101 {
                return Ok(resp);
            }

            let status = resp.status_code();
            let body_text = resp.text().await.unwrap_or_default();
            let fresh_resp = if (200..300).contains(&status) {
                let mut r = Response::ok(body_text)?;
                if status != 200 {
                    r = r.with_status(status);
                }
                r
            } else {
                Response::error(body_text, status)?
            };

            with_cors(fresh_resp)
        })
        .get_async("/ice", |_req, ctx| async move {
            handle_ice_config(&ctx.env).await
        })
        .post_async("/ice", |_req, ctx| async move {
            handle_ice_config(&ctx.env).await
        })
        .get("/health", |_, _| with_cors(Response::ok("ok")?))
        .run(req, env)
        .await
}
