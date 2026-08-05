//! Integration tests driving real in-process nodes over TCP loopback.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use shared::types::{Task, TaskId, TaskStatus, TaskType};
use thala::node::{Node, NodeConfig};

/// Grab an ephemeral port by binding then dropping a listener. `Node::new`
/// records the passed address as its advertised `listen_addr`, so we must hand
/// it a concrete port (not `:0`) for peer reconnection/gossip to work.
fn free_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn unique_temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("thala_it_{}_{}", tag, nanos))
}

fn test_config(tag: &str, models: Vec<String>, datasets: Vec<String>) -> NodeConfig {
    NodeConfig {
        peer_reconnection_interval: Duration::from_secs(3600),
        max_backoff_interval: Duration::from_secs(60),
        backoff_multiplier: 2.0,
        reconnection_retries_cap: 3,
        rpc_addr: None,
        data_dir: unique_temp_dir(tag),
        models,
        datasets,
        // short claim window to keep assignment tests fast
        claim_window: Duration::from_millis(500),
    }
}

/// Build a node, start its background loops, and return it with its listen addr.
async fn spawn_node(
    tag: &str,
    bootstrap: Option<SocketAddr>,
    models: Vec<String>,
    datasets: Vec<String>,
) -> (Arc<Node>, SocketAddr) {
    let addr = free_addr();
    let node = Node::new(addr, bootstrap, test_config(tag, models, datasets))
        .await
        .unwrap();
    let handle = node.clone();
    tokio::spawn(async move {
        let _ = handle.start().await;
    });
    (node, addr)
}

/// Poll `cond` until it returns true or the timeout elapses.
async fn wait_until(timeout: Duration, mut cond: impl AsyncFnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if cond().await {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

const TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn two_nodes_complete_handshake() {
    let (a, a_addr) = spawn_node("handshake_a", None, vec![], vec![]).await;
    let (b, _) = spawn_node("handshake_b", Some(a_addr), vec![], vec![]).await;

    assert!(
        wait_until(TIMEOUT, async || a
            .connected_peer_ids()
            .await
            .contains(&b.id()))
        .await,
        "A should have B in its connections"
    );
    assert!(
        wait_until(TIMEOUT, async || b
            .connected_peer_ids()
            .await
            .contains(&a.id()))
        .await,
        "B should have A in its connections"
    );
}

#[tokio::test]
async fn known_peers_gossip_to_third_node() {
    let (a, a_addr) = spawn_node("gossip_a", None, vec![], vec![]).await;
    let (b, b_addr) = spawn_node("gossip_b", Some(a_addr), vec![], vec![]).await;

    // Ensure A<->B handshake is done so B knows about A before C joins.
    assert!(
        wait_until(TIMEOUT, async || b.known_peer_ids().await.contains(&a.id())).await,
        "B should learn about A"
    );

    let (c, _) = spawn_node("gossip_c", Some(b_addr), vec![], vec![]).await;

    assert!(
        wait_until(TIMEOUT, async || c.known_peer_ids().await.contains(&a.id())).await,
        "C should learn about A via gossip from B"
    );
}

#[tokio::test]
async fn task_announcement_produces_claim() {
    let (coordinator, coord_addr) = spawn_node("claim_coord", None, vec![], vec![]).await;
    let (worker, _) = spawn_node(
        "claim_worker",
        Some(coord_addr),
        vec!["m".to_string()],
        vec!["d".to_string()],
    )
    .await;

    // Both directions of the connection must exist: coordinator broadcasts to
    // the worker, and the worker sends its claim back over the same link.
    assert!(
        wait_until(TIMEOUT, async || coordinator
            .connected_peer_ids()
            .await
            .contains(&worker.id()))
        .await,
        "coordinator should be connected to worker"
    );
    assert!(
        wait_until(TIMEOUT, async || worker
            .connected_peer_ids()
            .await
            .contains(&coordinator.id()))
        .await,
        "worker should be connected to coordinator"
    );

    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let task = Task::new(
        TaskId::new(),
        TaskType::Benchmark {
            model: "m".to_string(),
            dataset: "d".to_string(),
        },
        expires,
    );
    let task_id = task.id();

    coordinator.broadcast_task(task).await.unwrap();

    assert!(
        wait_until(TIMEOUT, async || coordinator
            .assigned_worker(&task_id)
            .await
            == Some(worker.id()))
        .await,
        "coordinator should assign the task to the claiming worker"
    );
    assert!(
        wait_until(TIMEOUT, async || coordinator
            .coordinated_task_status(&task_id)
            .await
            == Some(TaskStatus::Running))
        .await,
        "coordinator should mark the task Running after assignment"
    );
    assert!(
        wait_until(TIMEOUT, async || worker
            .pending_task_status(&task_id)
            .await
            == Some(TaskStatus::Running))
        .await,
        "worker should mark its pending task Running after receiving TaskAssignment"
    );
}

#[tokio::test]
async fn coordinator_assigns_exactly_one_of_two_claimants() {
    let (coordinator, coord_addr) = spawn_node("assign_coord", None, vec![], vec![]).await;
    let (worker_a, _) = spawn_node(
        "assign_worker_a",
        Some(coord_addr),
        vec!["m".to_string()],
        vec!["d".to_string()],
    )
    .await;
    let (worker_b, _) = spawn_node(
        "assign_worker_b",
        Some(coord_addr),
        vec!["m".to_string()],
        vec!["d".to_string()],
    )
    .await;

    // Both workers must have bidirectional connections with the coordinator so
    // they receive the announcement and can send claims back.
    for worker in [&worker_a, &worker_b] {
        assert!(
            wait_until(TIMEOUT, async || coordinator
                .connected_peer_ids()
                .await
                .contains(&worker.id()))
            .await,
            "coordinator should be connected to worker"
        );
        assert!(
            wait_until(TIMEOUT, async || worker
                .connected_peer_ids()
                .await
                .contains(&coordinator.id()))
            .await,
            "worker should be connected to coordinator"
        );
    }

    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let task = Task::new(
        TaskId::new(),
        TaskType::Benchmark {
            model: "m".to_string(),
            dataset: "d".to_string(),
        },
        expires,
    );
    let task_id = task.id();

    coordinator.broadcast_task(task).await.unwrap();

    assert!(
        wait_until(TIMEOUT, async || coordinator
            .assigned_worker(&task_id)
            .await
            .is_some())
        .await,
        "coordinator should assign the task after the claim window"
    );

    let winner = coordinator.assigned_worker(&task_id).await.unwrap();
    assert!(
        winner == worker_a.id() || winner == worker_b.id(),
        "assignment should go to one of the claiming workers"
    );

    assert_eq!(
        coordinator.coordinated_task_status(&task_id).await,
        Some(TaskStatus::Running),
        "coordinator should mark the task Running"
    );

    // The assignment must be stable: late/duplicate claims must not change it.
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(
        coordinator.assigned_worker(&task_id).await,
        Some(winner),
        "assignment should not change after the claim window closes"
    );

    // Exactly one worker was told to run the task.
    let (winner_node, loser_node) = if winner == worker_a.id() {
        (&worker_a, &worker_b)
    } else {
        (&worker_b, &worker_a)
    };
    assert!(
        wait_until(TIMEOUT, async || winner_node
            .pending_task_status(&task_id)
            .await
            == Some(TaskStatus::Running))
        .await,
        "winning worker should mark the task Running"
    );
    assert_eq!(
        loser_node.pending_task_status(&task_id).await,
        Some(TaskStatus::Pending),
        "losing worker should still hold the task as Pending"
    );
}
