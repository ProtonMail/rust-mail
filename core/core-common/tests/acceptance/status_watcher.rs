use futures::FutureExt;
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::session::{Config, Session};
use proton_core_api::status_observer::StatusObserver;
use proton_core_api::status_watcher::StatusWatcher;
use proton_core_common::test_utils::test_context::MockApiEnv;
use proton_core_common::test_utils::utils::{catch_all, mock_auth_endpoints};
use std::time::Duration;
use test_case::test_case;
use tokio::time::sleep;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn status_watcher(millis: u64) -> StatusWatcher {
    let mut sw = StatusWatcher::with_observer(StatusObserver::test());
    sw.set_up_to_date(Duration::from_millis(millis));
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
    mock_auth_endpoints(&mock_server).await;
    let mock_env = MockApiEnv::new(mock_server.uri()).with_path("/api");
    let api_config = Config::for_env(mock_env);
    let api_1 = Session::builder()
        .with_config(&api_config)
        .build()
        .await
        .unwrap();
    let api_2 = Session::builder()
        .with_config(&api_config)
        .build()
        .await
        .unwrap();
    let api_3 = api_1.clone();

    Mock::given(method("GET"))
        .and(path(r"/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock_server)
        .await;

    catch_all(&mock_server).await;

    // Services start with the assumption that the connection is online, so
    // let's wait until they notice the meme highway is actually turned off now:
    api_1.wait_for_offline().await;
    api_2.wait_for_offline().await;
    api_3.wait_for_offline().await;

    assert_eq!(api_1.status().await, ConnectionStatus::ServerUnreachable);
    assert_eq!(api_2.status().await, ConnectionStatus::ServerUnreachable);
    assert_eq!(api_3.status().await, ConnectionStatus::ServerUnreachable);

    // Now let's pretend the connection went back up:
    mock_server.reset().await;

    Mock::given(method("GET"))
        .and(path(r"/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(200))
        // Due to fact that two sessions are build separatly it spawns 2 tasks
        // which may call the server once or twice x2 (2-4 requests)
        .expect(1..=4)
        .mount(&mock_server)
        .await;

    catch_all(&mock_server).await;

    // ... let's wait for any of the APIs to notice:
    api_1.wait_for_online().await;

    // ... and let's assert.
    //
    // Crucially, without waiting for `api_2` and `api_3` - since they are
    // supposed to share the same connection status, waiting for either one of
    // them should be sufficient to observe the same state across all three.
    for api in [&api_1, &api_2, &api_3] {
        // We use `.now_or_never()` to make sure that `api.status()` uses the
        // cached value instead of, say, sending a new request and waiting for
        // it to complete
        assert_eq!(api.status().now_or_never(), Some(ConnectionStatus::Online));
    }
}

#[tokio::test]
async fn make_another_request_when_stale() {
    let mock_server = MockServer::start().await;
    let api_path = random_path();
    mock_auth_endpoints(&mock_server).await;
    let mock_env = MockApiEnv::new(mock_server.uri()).with_path(&api_path);
    let api_config = Config::for_env(mock_env);
    let status = status_watcher(500);
    let api = Session::builder()
        .with_config(api_config)
        .with_status(status)
        .build()
        .await
        .unwrap();

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..=2)
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
    mock_auth_endpoints(&mock_server).await;
    let mock_env = MockApiEnv::new(mock_server.uri()).with_path(&api_path);
    let api_config = Config::for_env(mock_env);
    let status = status_watcher(1000);
    let api = Session::builder()
        .with_config(api_config)
        .with_status(status)
        .build()
        .await
        .unwrap();

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(500)))
        .expect(1..=3)
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

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn wait_for_online() {
    let mock_server = MockServer::start().await;
    let api_path = random_path();
    mock_auth_endpoints(&mock_server).await;
    let mock_env = MockApiEnv::new(mock_server.uri()).with_path(&api_path);
    let api_config = Config::for_env(mock_env);
    let status = status_watcher(500);
    let api = Session::builder()
        .with_config(api_config)
        .with_status(status)
        .build()
        .await
        .unwrap();

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;
    // Give some time for a server to start
    sleep(Duration::from_millis(200)).await;

    assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);

    // Restart server
    mock_server.reset().await;

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;

    api.wait_for_online().await;
    assert_eq!(api.status().await, ConnectionStatus::Online);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn multiple_subscribers() {
    let mock_server = MockServer::start().await;
    let api_path = random_path();
    mock_auth_endpoints(&mock_server).await;
    let mock_env = MockApiEnv::new(mock_server.uri()).with_path(&api_path);
    let api_config = Config::for_env(mock_env);
    let status = status_watcher(500);
    let api = Session::builder()
        .with_config(api_config)
        .with_status(status)
        .build()
        .await
        .unwrap();

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;
    // Give some time for a server to start
    sleep(Duration::from_millis(200)).await;

    assert_eq!(api.status().await, ConnectionStatus::ServerUnreachable);

    // Restart server
    mock_server.reset().await;

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;

    let mut subscriber_1 = api.status_changes();
    let subscriber_2 = api.status_changes();

    while !subscriber_1.has_changed().unwrap() {
        sleep(Duration::from_millis(100)).await;
    }

    assert!(subscriber_1.has_changed().unwrap());
    assert!(subscriber_2.has_changed().unwrap());

    assert!(subscriber_1.changed().await.is_ok());

    assert!(!subscriber_1.has_changed().unwrap());
    assert!(subscriber_2.has_changed().unwrap());

    assert_eq!(api.status().await, ConnectionStatus::Online);
}

#[test_case(200, ConnectionStatus::Online; "TEST 1 - 200 Ok")]
#[test_case(201, ConnectionStatus::Online; "TEST 2 - 201 Created")]
#[test_case(204, ConnectionStatus::Online; "TEST 3 - 204 No Content")]
#[test_case(304, ConnectionStatus::Online; "TEST 4 - 304 Not Modified")]
#[test_case(400, ConnectionStatus::Online; "TEST 5 - 400 Bad Request")]
// #[test_case(401, ConnectionStatus::Online; "TEST 6 - 401 Unauthorized")] // Problematic test case - layer handles it as Offline which is not true
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
    mock_auth_endpoints(&mock_server).await;
    let mock_env = MockApiEnv::new(mock_server.uri()).with_path(&api_path);
    let api_config = Config::for_env(mock_env);

    let api = Session::builder()
        .with_config(api_config)
        .with_status(status_watcher(500))
        .build()
        .await
        .unwrap();

    Mock::given(method("GET"))
        .and(path(format!("{api_path}/core/v4/tests/ping")))
        .respond_with(ResponseTemplate::new(http_code))
        .expect(1..=2)
        .mount(&mock_server)
        .await;
    catch_all(&mock_server).await;
    // Give some time for a server to start
    sleep(Duration::from_millis(200)).await;

    // Check if all this calls trigger a single request
    assert_eq!(api.status().await, expected_status);
}
