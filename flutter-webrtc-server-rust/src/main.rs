use anyhow::Result;
use axum::{
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, get_service},
    Router,
};
use log::{error, info};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};

mod modules;

use modules::{
    config::Config,
    signaling::Signaler,
    turn_server::TurnServer,
};

#[derive(Debug, Deserialize)]
struct TurnQuery {
    service: String,
    username: String,
}

#[derive(Clone)]
struct AppState {
    signaler: Arc<Signaler>,
    config: Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let config = Config::load_from_file("configs/config.ini")?;
    info!("Loaded configuration: {:?}", config);

    let signaler = Arc::new(Signaler::new(config.turn.clone()));
    let mut turn_server = TurnServer::new(config.turn.clone(), signaler.clone());

    // Start TURN server
    if let Err(e) = turn_server.start().await {
        error!("Failed to start TURN server: {}", e);
    }

    let app_state = AppState {
        signaler: signaler.clone(),
        config: config.clone(),
    };

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/api/turn", get(turn_credentials_handler))
        .nest_service("/", get_service(ServeDir::new(&config.general.html_root)))
        .layer(
            ServiceBuilder::new()
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods(Any)
                        .allow_headers(Any),
                )
        )
        .with_state(app_state);

    let bind_addr: SocketAddr = format!("{}:{}", config.general.bind, config.general.port).parse()?;

    info!("Flutter WebRTC Server listening on: {}", bind_addr);

    // For simplicity, start with HTTP server
    // TLS can be added later by configuring a reverse proxy like nginx
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        state.signaler.handle_websocket(socket).await;
    })
}

async fn turn_credentials_handler(
    Query(params): Query<TurnQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if params.service != "turn" {
        return (StatusCode::BAD_REQUEST, Json("Invalid service")).into_response();
    }

    match state.signaler.generate_turn_credentials(&params.username) {
        Ok(credentials) => Json(credentials).into_response(),
        Err(e) => {
            error!("Failed to generate TURN credentials: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json("Failed to generate credentials")).into_response()
        }
    }
}

