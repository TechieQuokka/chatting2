mod account;
mod app;
mod chat;
mod crypto;
mod friends;
mod i18n;
mod invite;
mod network;
mod protocol;
mod room;
mod transfer;
mod tui;

#[tokio::main]
async fn main() {
    // ── Graceful Shutdown (Ctrl-C / SIGINT) ───────────────────────────────────
    //
    // 완전한 통합에서는:
    //   1. account 로그인 → (Identity, Config) 획득
    //   2. build_swarm(&identity, &net_config)
    //   3. tokio::spawn(network::swarm::run_event_loop(...))
    //   4. AppCore::new(...).run(shutdown_rx).await
    // 를 순서대로 실행한다.
    //
    // 현재는 진입점 구조만 정의하고, Ctrl-C 수신 시 정상 종료한다.

    println!("chatting2 — 시작 중...");

    tokio::signal::ctrl_c()
        .await
        .expect("Ctrl-C 핸들러 등록 실패");

    println!("종료 중...");
}
