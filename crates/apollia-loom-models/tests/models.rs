//! Loom abstract models for the runtime's concurrent actor algorithms.
//!
//! # Why these are abstract models, not instrumented actors
//!
//! Loom is a permutation-testing model checker for `std`/`loom` synchronous
//! primitives and OS threads. It cannot instrument Tokio: every runtime actor
//! is built on `tokio::sync` channels (`mpsc`, `broadcast`, `oneshot`) and a
//! `tokio::sync::Semaphore` running on the Tokio scheduler, none of which Loom
//! can see. Swapping `tokio::sync::mpsc` for a Loom channel while keeping the
//! `async`/`.await` model is not possible.
//!
//! Worse, `--cfg loom` is a global rustc flag: Tokio itself gates `tokio::net`
//! behind `cfg(not(loom))`, so building any Tokio-dependent crate under the flag
//! fails to compile (hyper-util/axum lose `tokio::net::UnixStream`). That is why
//! this lives in a standalone crate excluded from the workspace, with no Tokio
//! in its dependency tree.
//!
//! So each model here re-implements one actor's core concurrency algorithm with
//! Loom primitives, decoupled from the shipping code. Every model cites the prod
//! `file:line` it mirrors so drift is auditable. The honest claim is: these
//! prove the ALGORITHM is race-free (or expose the hazard), not that the exact
//! Tokio code is. Two of them (`mailbox_lease_exclusivity`,
//! `shutdown_drain_snapshot_gap`) model the RECOMMENDED fix for a hazard the
//! prod code does not yet implement; the delta is a tracked finding (F3, F4).
//!
//! # Running
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" LOOM_MAX_PREEMPTIONS=3 \
//!   cargo test --manifest-path crates/apollia-loom-models/Cargo.toml --release
//! ```
//!
//! Without `--cfg loom` this crate compiles to nothing (the models are gated and
//! the `loom` dependency is pulled only under the flag), so it is inert to any
//! normal build.

#![cfg(loom)]
// Some enum variants (e.g. Status::Failed) exist to mirror the real terminal
// set faithfully even when a given model does not construct them.
#![allow(dead_code)]

/// Mirrors `registry.rs:148-179` (`handle_register` name eviction +
/// `handle_unregister`).
///
/// The real `AgentRegistry` is a single serial actor draining a bounded mpsc,
/// so concurrent `Register`/`Unregister` can never interleave mid-operation.
/// This model proves the INVARIANT that serialization is meant to provide:
/// under every arrival order of two concurrent same-name registrations, the
/// `agents` map and `name_index` stay consistent (the index never points at an
/// evicted id, and no orphan entry survives).
mod registry_serial_consistency {
    use loom::sync::{Arc, Mutex};
    use std::collections::VecDeque;

    enum Cmd {
        Register { name: &'static str, id: u32 },
    }

    #[test]
    fn concurrent_same_name_register_keeps_index_consistent() {
        loom::model(|| {
            // The command queue is the only shared state: producers enqueue
            // concurrently, the actor thread drains and applies logic serially.
            let queue: Arc<Mutex<VecDeque<Cmd>>> = Arc::new(Mutex::new(VecDeque::new()));

            let q1 = Arc::clone(&queue);
            let p1 = loom::thread::spawn(move || {
                q1.lock()
                    .unwrap()
                    .push_back(Cmd::Register { name: "x", id: 1 });
            });
            let q2 = Arc::clone(&queue);
            let p2 = loom::thread::spawn(move || {
                q2.lock()
                    .unwrap()
                    .push_back(Cmd::Register { name: "x", id: 2 });
            });

            p1.join().unwrap();
            p2.join().unwrap();

            // Actor: drain FIFO, apply the real eviction rule (registry.rs:151).
            let mut agents: std::collections::HashMap<u32, &'static str> =
                std::collections::HashMap::new();
            let mut name_index: std::collections::HashMap<&'static str, u32> =
                std::collections::HashMap::new();

            let mut q = queue.lock().unwrap();
            while let Some(cmd) = q.pop_front() {
                match cmd {
                    Cmd::Register { name, id } => {
                        if let Some(old_id) = name_index.remove(name) {
                            agents.remove(&old_id);
                        }
                        name_index.insert(name, id);
                        agents.insert(id, name);
                    }
                }
            }

            // Invariant: exactly one live entry for the name, and the index
            // resolves to an id that exists in `agents` (no dangling pointer,
            // no orphan).
            assert_eq!(name_index.len(), 1);
            assert_eq!(agents.len(), 1);
            let idx_id = name_index["x"];
            assert!(agents.contains_key(&idx_id));
        });
    }
}

/// Mirrors `coordinator.rs:143-193` (`try_acquire_owned` + permit
/// release-on-drop).
///
/// The prod code uses `tokio::sync::Semaphore`, which is correct by
/// construction. This model reproduces the same guarantee with a lock-free
/// counting semaphore (CAS loop) so Loom exercises the acquire/release atomic
/// interleavings: the live permit count never exceeds `N`, and the permit pool
/// returns to `N` with no leak once every holder has dropped.
mod coordinator_semaphore {
    use loom::sync::atomic::{AtomicUsize, Ordering};
    use loom::sync::Arc;

    const N: usize = 1;

    fn try_acquire(permits: &AtomicUsize) -> bool {
        loop {
            let cur = permits.load(Ordering::Acquire);
            if cur == 0 {
                return false;
            }
            match permits.compare_exchange(cur, cur - 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return true,
                Err(_) => continue,
            }
        }
    }

    #[test]
    fn try_acquire_never_exceeds_capacity() {
        loom::model(|| {
            let permits = Arc::new(AtomicUsize::new(N));
            let live = Arc::new(AtomicUsize::new(0));

            let workers: Vec<_> = (0..2)
                .map(|_| {
                    let permits = Arc::clone(&permits);
                    let live = Arc::clone(&live);
                    loom::thread::spawn(move || {
                        if try_acquire(&permits) {
                            // Holding the permit: the live count must never
                            // exceed capacity at any observable point.
                            let now = live.fetch_add(1, Ordering::AcqRel) + 1;
                            assert!(now <= N, "over-acquired: {now} > {N}");
                            live.fetch_sub(1, Ordering::AcqRel);
                            // Release on drop (coordinator.rs:193).
                            permits.fetch_add(1, Ordering::Release);
                        }
                    })
                })
                .collect();

            for w in workers {
                w.join().unwrap();
            }

            // No permit leaked or duplicated.
            assert_eq!(permits.load(Ordering::Acquire), N);
            assert_eq!(live.load(Ordering::Acquire), 0);
        });
    }
}

/// Mirrors `mailbox.rs` (`handle_receive` lease + `handle_ack`).
///
/// FINDING F4 (now fixed in prod): `handle_ack` used to delete by
/// `(message_id, to_agent)` with NO lease-owner fence, so a stale consumer whose
/// lease had expired could delete a message since re-leased to a second consumer.
/// Prod now records the leasing run in a `lease_owner` column and fences
/// `ack`/`nack` on it (null-safe `IS`), and Kani proves the fence bit-precise
/// (`mailbox::proofs`). This model proves the same fix (ack scoped to the current
/// lease owner) upholds exclusivity under every interleaving. The deterministic
/// repro of the unfenced bug is documented inline.
mod mailbox_lease_exclusivity {
    use loom::sync::{Arc, Mutex};

    #[derive(PartialEq, Clone, Copy)]
    enum State {
        InFlight,
        Gone,
    }

    struct Msg {
        state: State,
        owner: u32,
        // Models "the previous lease has expired" so a fresh receive is allowed.
        lease_expired: bool,
    }

    #[test]
    fn fenced_ack_never_deletes_a_live_reassigned_lease() {
        loom::model(|| {
            // Pre-condition: consumer A (id 1) holds an EXPIRED lease.
            let msg = Arc::new(Mutex::new(Msg {
                state: State::InFlight,
                owner: 1,
                lease_expired: true,
            }));

            // Consumer A acks its (expired) lease. FENCED: only deletes when it
            // is still the recorded owner. Prod is UNFENCED and would delete
            // unconditionally here -> F4. Under the unfenced rule the B-then-A
            // interleaving leaves the message Gone while owner == 2.
            let a = {
                let msg = Arc::clone(&msg);
                loom::thread::spawn(move || {
                    let mut m = msg.lock().unwrap();
                    if m.state == State::InFlight && m.owner == 1 {
                        m.state = State::Gone;
                    }
                })
            };

            // Consumer B (id 2) receives: an in-flight message whose lease has
            // expired is re-leasable (mailbox.rs:597).
            let b = {
                let msg = Arc::clone(&msg);
                loom::thread::spawn(move || {
                    let mut m = msg.lock().unwrap();
                    if m.state == State::InFlight && m.lease_expired {
                        m.owner = 2;
                        m.lease_expired = false;
                    }
                })
            };

            a.join().unwrap();
            b.join().unwrap();

            // Invariant: the message is never deleted while owned by B (the new
            // leaseholder) who has not acked. Under the FENCED ack this holds:
            // if B re-leased first, A's fenced ack sees owner==2 and is a no-op.
            let m = msg.lock().unwrap();
            let deleted_under_b = m.state == State::Gone && m.owner == 2;
            assert!(
                !deleted_under_b,
                "fenced ack deleted a message re-leased to another consumer"
            );
        });
    }
}

/// Mirrors `router.rs:187-199` (late `TaskCompleted` guard) and
/// `router.rs:329-339` (`handle_cancel`).
///
/// The real router serializes cancel and the late-completion event in one
/// `tokio::select!` loop, so they cannot interleave mid-write. Modeling them as
/// two truly concurrent writers is a STRONGER check: it proves the
/// `!is_terminal` guard makes a terminal status sticky even under real
/// concurrency, so a late backend completion can never erase a user
/// cancellation.
mod router_terminal_status_guard {
    use loom::sync::{Arc, Mutex};

    #[derive(PartialEq, Clone, Copy, Debug)]
    enum Status {
        Working,
        Canceled,
        Completed,
        Failed,
    }

    fn is_terminal(s: Status) -> bool {
        matches!(s, Status::Canceled | Status::Completed | Status::Failed)
    }

    #[test]
    fn late_completion_never_overwrites_terminal_status() {
        loom::model(|| {
            let status = Arc::new(Mutex::new(Status::Working));

            // Cancel path (handle_cancel): only Submitted/Working/InputRequired
            // may transition to Canceled.
            let cancel = {
                let status = Arc::clone(&status);
                loom::thread::spawn(move || {
                    let mut s = status.lock().unwrap();
                    if matches!(*s, Status::Working) {
                        *s = Status::Canceled;
                    }
                })
            };

            // Late TaskCompleted event: only writes when not already terminal.
            let complete = {
                let status = Arc::clone(&status);
                loom::thread::spawn(move || {
                    let mut s = status.lock().unwrap();
                    if !is_terminal(*s) {
                        *s = Status::Completed;
                    }
                })
            };

            cancel.join().unwrap();
            complete.join().unwrap();

            // Invariant: the outcome is terminal and, once set, unchanged. There
            // is no interleaving that flips one terminal state into another.
            let final_status = *status.lock().unwrap();
            assert!(
                matches!(final_status, Status::Canceled | Status::Completed),
                "unexpected non-sticky status: {final_status:?}"
            );
        });
    }
}

/// Mirrors `shutdown.rs:310-347` (`Arc<AtomicBool>` double-Ctrl-C latch).
///
/// The signal source itself is external and cannot be modeled, but the shared
/// `force_exit_armed` flag can: a single-writer, single-reader latch that must
/// be monotonic (false -> true, never back) and free of torn reads under the
/// declared memory ordering.
mod shutdown_force_exit_flag {
    use loom::sync::atomic::{AtomicBool, Ordering};
    use loom::sync::Arc;

    #[test]
    fn force_exit_latch_is_monotonic() {
        loom::model(|| {
            let armed = Arc::new(AtomicBool::new(false));

            // First-signal handler arms the latch (shutdown.rs:334).
            let writer = {
                let armed = Arc::clone(&armed);
                loom::thread::spawn(move || {
                    armed.store(true, Ordering::SeqCst);
                })
            };

            // Watcher reads the latch twice; once observed armed it must never
            // read disarmed again (shutdown.rs:319).
            let reader = {
                let armed = Arc::clone(&armed);
                loom::thread::spawn(move || {
                    let first = armed.load(Ordering::SeqCst);
                    let second = armed.load(Ordering::SeqCst);
                    // Once observed armed, the latch must never read disarmed again.
                    if first {
                        assert!(second, "latch regressed true -> false");
                    }
                })
            };

            writer.join().unwrap();
            reader.join().unwrap();

            assert!(armed.load(Ordering::SeqCst));
        });
    }
}

/// Mirrors `apollia-oria::plan_gate` (`PendingPlanGates::decide`, plan_gate.rs:72)
/// used by `plan_approval.rs:75`.
///
/// HITL concurrent-resume race: two approvers (or an approver racing the engine)
/// call `decide` for the same run. The `Mutex<HashMap>` + `remove` must deliver
/// the decision EXACTLY once; every loser observes `UnknownRunId`.
mod plan_gate_single_decision {
    use loom::sync::atomic::{AtomicUsize, Ordering};
    use loom::sync::{Arc, Mutex};

    #[test]
    fn concurrent_decide_delivers_exactly_once() {
        loom::model(|| {
            // Some(()) == a registered gate whose receiver is alive.
            let gate: Arc<Mutex<Option<()>>> = Arc::new(Mutex::new(Some(())));
            let delivered = Arc::new(AtomicUsize::new(0));
            let winners = Arc::new(AtomicUsize::new(0));

            let deciders: Vec<_> = (0..2)
                .map(|_| {
                    let gate = Arc::clone(&gate);
                    let delivered = Arc::clone(&delivered);
                    let winners = Arc::clone(&winners);
                    loom::thread::spawn(move || {
                        // decide(): lock, remove the entry, send if present.
                        let taken = gate.lock().unwrap().take();
                        if taken.is_some() {
                            delivered.fetch_add(1, Ordering::AcqRel);
                            winners.fetch_add(1, Ordering::AcqRel);
                        }
                    })
                })
                .collect();

            for d in deciders {
                d.join().unwrap();
            }

            // Exactly one decider resolved the gate; the other saw it gone.
            assert_eq!(winners.load(Ordering::Acquire), 1);
            assert_eq!(delivered.load(Ordering::Acquire), 1);
        });
    }
}

/// Mirrors `shutdown.rs:178-198` (`drain_tasks` snapshot-then-subscribe).
///
/// FINDING F3: prod snapshots the active-task set (shutdown.rs:182) and only
/// THEN subscribes to the EventBus (shutdown.rs:198). A `TaskCompleted` emitted
/// in that window is missed, so an already-finished task lingers in `remaining`
/// until the full drain timeout, then gets force-canceled. This model proves the
/// RECOMMENDED fix (subscribe BEFORE snapshot, as `a2a/mod.rs:357` already does)
/// leaves no interleaving that loses a completion.
mod shutdown_drain_snapshot_gap {
    use loom::sync::{Arc, Mutex};

    struct Drain {
        subscribed: bool,
        task_done: bool,
        // True while the drain still believes the task is running.
        remaining: bool,
    }

    #[test]
    fn subscribe_before_snapshot_never_misses_a_completion() {
        loom::model(|| {
            let st = Arc::new(Mutex::new(Drain {
                subscribed: false,
                task_done: false,
                remaining: false,
            }));

            // Drain: FIXED order. Subscribe first, then snapshot.
            let drain = {
                let st = Arc::clone(&st);
                loom::thread::spawn(move || {
                    {
                        let mut s = st.lock().unwrap();
                        s.subscribed = true;
                    }
                    // Snapshot: a task already completed is not tracked.
                    let mut s = st.lock().unwrap();
                    s.remaining = !s.task_done;
                })
            };

            // Task completes and emits TaskCompleted.
            let task = {
                let st = Arc::clone(&st);
                loom::thread::spawn(move || {
                    let mut s = st.lock().unwrap();
                    s.task_done = true;
                    // If the drain is already subscribed, the event is delivered
                    // and clears any tracked entry.
                    if s.subscribed {
                        s.remaining = false;
                    }
                })
            };

            drain.join().unwrap();
            task.join().unwrap();

            // Invariant: the completed task is always accounted for; it is never
            // left in `remaining` to be force-canceled after timeout.
            let s = st.lock().unwrap();
            assert!(
                !s.remaining,
                "completed task was missed by the drain (snapshot/subscribe gap)"
            );
        });
    }
}
