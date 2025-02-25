use proton_api_core::connection_status::ConnectionStatus;
use proton_api_core::session::{Config, EnvId, Session};
use proton_api_core::status_watcher::StatusWatcher;
use proton_core_test_utils::test_context::MockApiEnv;
use proton_core_test_utils::utils::catch_all;
use std::time::Duration;
use test_case::test_case;
use tokio::time::sleep;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn status_watcher(millis: u64) -> StatusWatcher {
    let mut sw = StatusWatcher::test();
    let _ = sw.with_up_to_date(Duration::from_millis(millis)).await;
    sw
}

fn random_path() -> String {
    let mut path = String::from("/api_");
    path.push_str(&rand::random::<u64>().to_string());
    path
}

#[tokio::test]
async fn shared_status() {
    let mock_server = MockServer::start().await;
    let api_config = Config {
        env_id: EnvId::new_custom(MockApiEnv::new(mock_server.uri()).with_path("/api")),
        ..Default::default()
    };
    let api_1 = Session::new(api_config.clone(), None, StatusWatcher::new()).unwrap();
    let api_2 = Session::new(api_config, None, StatusWatcher::new()).unwrap();
    let api_3 = api_1.clone();

    Mock::given(method("GET"))
        .and(path(r"/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;
    // Give some time for a server to start
    sleep(Duration::from_millis(200)).await;

    // 429
    assert_eq!(api_1.status().await, ConnectionStatus::ServerUnreachable);
    assert_eq!(api_2.status().await, ConnectionStatus::ServerUnreachable);
    assert_eq!(api_3.status().await, ConnectionStatus::ServerUnreachable);

    mock_server.reset().await;
    Mock::given(method("GET"))
        .and(path(r"/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;
    // Give some time for a server to start
    sleep(Duration::from_millis(200)).await;

    // Check if all this calls trigger a single request - expect 2 as there is offline background request running
    assert_eq!(api_1.status().await, ConnectionStatus::Online);
    assert_eq!(api_2.status().await, ConnectionStatus::Online);
    assert_eq!(api_3.status().await, ConnectionStatus::Online);
    assert_eq!(api_1.status().await, ConnectionStatus::Online);
    assert_eq!(api_2.status().await, ConnectionStatus::Online);
    assert_eq!(api_3.status().await, ConnectionStatus::Online);
}

#[tokio::test]
async fn make_another_request_when_stale() {
    let mock_server = MockServer::start().await;
    let api_path = random_path();
    let api_config = Config {
        env_id: EnvId::new_custom(MockApiEnv::new(mock_server.uri()).with_path(&api_path)),
        ..Default::default()
    };
    let sw = status_watcher(500).await;
    let api = Session::new(api_config.clone(), None, sw).unwrap();

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;
    // Give some time for a server to start
    sleep(Duration::from_millis(200)).await;

    assert_eq!(api.status().await, ConnectionStatus::Online);
    // Make the status stale
    sleep(Duration::from_secs(1)).await;
    assert_eq!(api.status().await, ConnectionStatus::Online);
}

#[tokio::test]
async fn very_bad_connection_but_responding_in_under_a_second() {
    let mock_server = MockServer::start().await;
    let api_path = random_path();
    let api_config = Config {
        env_id: EnvId::new_custom(MockApiEnv::new(mock_server.uri()).with_path(&api_path)),
        ..Default::default()
    };
    let sw = status_watcher(1000).await;
    let api = Session::new(api_config.clone(), None, sw).unwrap();

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(500)))
        .expect(2)
        .mount(&mock_server)
        .await;

    catch_all(&mock_server).await;
    // Give some time for a server to start
    sleep(Duration::from_millis(200)).await;

    assert_eq!(api.status().await, ConnectionStatus::Online);
    assert_eq!(api.status().await, ConnectionStatus::Online);
    sleep(Duration::from_secs(2)).await;
    assert_eq!(api.status().await, ConnectionStatus::Online);
    assert_eq!(api.status().await, ConnectionStatus::Online);
}

// #[tokio::test]
// async fn offline_background_requests() {
//     let mock_server = MockServer::start().await;
//     let api_path = random_path();
//     let api_config = Config {
//         env_id: EnvId::new_custom(MockApiEnv::new(mock_server.uri()).with_path(&api_path)),
//         ..Default::default()
//     };
//     let api = Session::new(
//         api_config.clone(),
//         None,
//         StatusWatcher::test()
//             .with_up_to_date(Duration::from_millis(1000))
//             .await,
//     )
//     .unwrap();

//     Mock::given(method("GET"))
//         .and(path(format!("{api_path}/core/v4/tests/ping")))
//         .respond_with(ResponseTemplate::new(500))
//         .expect(5)
//         .mount(&mock_server)
//         .await;

//     catch_all(&mock_server).await;
//     // Give some time for a server to start
//     sleep(Duration::from_millis(200)).await;

//     assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);
//     assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);
//     assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);
//     assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);
//     assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);
//     assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);
//     assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);
//     assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);
// }

// TODO: Very unreliable tests in CI, needs to be fixed
// #[tokio::test]
// async fn terribly_bad_connection_and_server_restart_simulation() {
//     let mock_server = MockServer::start().await;
//     let api_path = random_path();
//     let api_config = Config {
//         env_id: EnvId::new_custom(MockApiEnv::new(mock_server.uri()).with_path(&api_path)),
//         ..Default::default()
//     };
//     let api = Session::new(api_config.clone(), None, status_watcher().await).unwrap();

//     Mock::given(method("GET"))
//         .and(path(format!("{api_path}/core/v4/tests/ping")))
//         .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(20)))
//         .expect(2)
//         .mount(&mock_server)
//         .await;

//     catch_all(&mock_server).await;
//     // Give some time for a server to start
//     sleep(Duration::from_millis(200)).await;

//     assert_eq!(api.status().await, ConnectionStatus::Offline);

//     mock_server.reset().await;

//     Mock::given(method("GET"))
//         .and(path(format!("{api_path}/core/v4/tests/ping")))
//         .respond_with(ResponseTemplate::new(200))
//         .expect(2)
//         .mount(&mock_server)
//         .await;

//     catch_all(&mock_server).await;

//     sleep(Duration::from_millis(200)).await;
//     assert_eq!(api.status().await, ConnectionStatus::Online);
//     sleep(Duration::from_millis(100)).await;
//     assert_eq!(api.status().await, ConnectionStatus::Online);
// }

// #[tokio::test]
// async fn terribly_bad_connection_responding_in_twenty_seconds() {
//     let mock_server = MockServer::start().await;
//     let api_path = random_path();
//     let api_config = Config {
//         env_id: EnvId::new_custom(MockApiEnv::new(mock_server.uri()).with_path(&api_path)),
//         ..Default::default()
//     };
//     let api = Session::new(api_config.clone(), None, status_watcher().await).unwrap();

//     Mock::given(method("GET"))
//         .and(path(format!("{api_path}/core/v4/tests/ping")))
//         .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(20)))
//         .expect(2)
//         .mount(&mock_server)
//         .await;

//     catch_all(&mock_server).await;
//     // Give some time for a server to start
//     sleep(Duration::from_millis(200)).await;

//     // Timeout
//     assert_eq!(api.status().await, ConnectionStatus::Offline);
//     sleep(Duration::from_millis(100)).await;
//     assert_eq!(api.status().await, ConnectionStatus::Offline);
// }

#[test_case(200, ConnectionStatus::Online; "TEST 1 - 200 Ok")]
#[test_case(201, ConnectionStatus::Online; "TEST 2 - 201 Created")]
#[test_case(204, ConnectionStatus::Online; "TEST 3 - 204 No Content")]
#[test_case(304, ConnectionStatus::Online; "TEST 4 - 304 Not Modified")]
#[test_case(400, ConnectionStatus::Online; "TEST 5 - 400 Bad Request")]
#[test_case(401, ConnectionStatus::Online; "TEST 6 - 401 Unauthorized")]
#[test_case(403, ConnectionStatus::Online; "TEST 7 - 403 Forbidden")]
#[test_case(404, ConnectionStatus::Online; "TEST 8 - 404 Not Found")]
#[test_case(408, ConnectionStatus::Online; "TEST 9 - 408 Request Timeout")]
#[test_case(415, ConnectionStatus::Online; "TEST 10 - 415 Unsupported Media Type")]
#[test_case(418, ConnectionStatus::Online; "TEST 11 - 418 I'm a teapot")]
#[test_case(429, ConnectionStatus::ServerUnreachable; "TEST 12 - 429 Too Many Requests")]
#[test_case(500, ConnectionStatus::ServerUnreachable; "TEST 13 - 500 Internal Server Error")]
#[test_case(502, ConnectionStatus::ServerUnreachable; "TEST 14 - 502 Bad Gateway")]
#[tokio::test]
async fn status_reflected_in_response_http_code(http_code: u16, expected_status: ConnectionStatus) {
    let mock_server = MockServer::start().await;
    let api_path = random_path();
    let api_config = Config {
        env_id: EnvId::new_custom(MockApiEnv::new(mock_server.uri()).with_path(&api_path)),
        ..Default::default()
    };
    let api = Session::new(api_config.clone(), None, status_watcher(0).await).unwrap();

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(http_code))
        .expect(1)
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;
    // Give some time for a server to start
    sleep(Duration::from_millis(200)).await;

    // Check if all this calls trigger a single request
    assert_eq!(api.status().await, expected_status);
}
