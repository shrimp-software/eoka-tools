use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Lane {
    Barrier,
    Page(String),
    Independent,
}

struct Waiting<T> {
    ticket: u64,
    lane: Lane,
    operation: Option<String>,
    after: Option<u64>,
    item: T,
}

struct Running {
    ticket: u64,
    lane: Lane,
    operation: Option<String>,
}

pub(super) struct Lanes<T> {
    waiting: VecDeque<Waiting<T>>,
    running: Vec<Running>,
    next_ticket: u64,
}

impl<T> Default for Lanes<T> {
    fn default() -> Self {
        Self {
            waiting: VecDeque::new(),
            running: Vec::new(),
            next_ticket: 0,
        }
    }
}

impl<T> Lanes<T> {
    pub(super) fn admit(&mut self, lane: Lane, operation: Option<String>, item: T) {
        self.enqueue(lane, operation, None, item);
    }

    pub(super) fn admit_after_operation(&mut self, operation: Option<String>, item: T) {
        let after = operation
            .as_deref()
            .and_then(|operation| self.latest_for_operation(operation));
        self.enqueue(Lane::Independent, operation, after, item);
    }

    fn enqueue(&mut self, lane: Lane, operation: Option<String>, after: Option<u64>, item: T) {
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        self.waiting.push_back(Waiting {
            ticket,
            lane,
            operation,
            after,
            item,
        });
    }

    fn latest_for_operation(&self, operation: &str) -> Option<u64> {
        let waiting = self
            .waiting
            .iter()
            .filter(|entry| entry.operation.as_deref() == Some(operation))
            .map(|entry| entry.ticket);
        let running = self
            .running
            .iter()
            .filter(|entry| entry.operation.as_deref() == Some(operation))
            .map(|entry| entry.ticket);
        waiting.chain(running).max()
    }

    fn outstanding(&self, ticket: u64) -> bool {
        self.running.iter().any(|entry| entry.ticket == ticket)
            || self.waiting.iter().any(|entry| entry.ticket == ticket)
    }

    pub(super) fn ready(&mut self) -> Vec<(u64, T)> {
        let mut started = Vec::new();
        if self.running.iter().any(|entry| entry.lane == Lane::Barrier) {
            return started;
        }
        let mut occupied: HashSet<Lane> = self
            .running
            .iter()
            .filter(|entry| matches!(entry.lane, Lane::Page(_)))
            .map(|entry| entry.lane.clone())
            .collect();
        let mut index = 0;
        while index < self.waiting.len() {
            let entry = &self.waiting[index];
            let barrier = entry.lane == Lane::Barrier;
            let startable = if barrier {
                index == 0 && self.running.is_empty()
            } else {
                !occupied.contains(&entry.lane)
                    && !entry.after.is_some_and(|ticket| self.outstanding(ticket))
            };
            if !startable {
                if barrier {
                    break;
                }
                if matches!(entry.lane, Lane::Page(_)) {
                    occupied.insert(entry.lane.clone());
                }
                index += 1;
                continue;
            }
            let Some(entry) = self.waiting.remove(index) else {
                break;
            };
            if matches!(entry.lane, Lane::Page(_)) {
                occupied.insert(entry.lane.clone());
            }
            self.running.push(Running {
                ticket: entry.ticket,
                lane: entry.lane,
                operation: entry.operation,
            });
            started.push((entry.ticket, entry.item));
            if barrier {
                break;
            }
        }
        started
    }

    pub(super) fn finish(&mut self, ticket: u64) {
        self.running.retain(|entry| entry.ticket != ticket);
    }

    pub(super) fn drain(&mut self) -> Vec<T> {
        self.waiting.drain(..).map(|entry| entry.item).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: &str) -> Lane {
        Lane::Page(id.to_owned())
    }

    fn names(started: Vec<(u64, &'static str)>) -> Vec<&'static str> {
        started.into_iter().map(|(_, name)| name).collect()
    }

    fn ticket(started: &[(u64, &'static str)], name: &str) -> u64 {
        started
            .iter()
            .find(|(_, item)| *item == name)
            .map(|(ticket, _)| *ticket)
            .unwrap()
    }

    #[test]
    fn pages_run_together_and_each_page_keeps_admission_order() {
        let mut lanes = Lanes::default();
        lanes.admit(page("a"), None, "a1");
        lanes.admit(page("b"), None, "b1");
        lanes.admit(page("a"), None, "a2");
        lanes.admit(page("a"), None, "a3");
        lanes.admit(page("b"), None, "b2");
        let first = lanes.ready();
        assert_eq!(names(first.clone()), ["a1", "b1"]);
        assert!(lanes.ready().is_empty());
        lanes.finish(ticket(&first, "b1"));
        let second = lanes.ready();
        assert_eq!(names(second.clone()), ["b2"]);
        lanes.finish(ticket(&second, "b2"));
        assert!(lanes.ready().is_empty());
        lanes.finish(ticket(&first, "a1"));
        let third = lanes.ready();
        assert_eq!(names(third.clone()), ["a2"]);
        lanes.finish(ticket(&third, "a2"));
        assert_eq!(names(lanes.ready()), ["a3"]);
    }

    #[test]
    fn a_stuck_page_never_delays_another_page_or_independent_work() {
        let mut lanes = Lanes::default();
        lanes.admit(page("stuck"), None, "never settles");
        assert_eq!(names(lanes.ready()), ["never settles"]);
        lanes.admit(page("stuck"), None, "queued behind");
        lanes.admit(page("other"), None, "read");
        lanes.admit(Lane::Independent, None, "new page");
        lanes.admit(Lane::Independent, None, "tabs");
        assert_eq!(names(lanes.ready()), ["read", "new page", "tabs"]);
        assert_eq!(lanes.drain(), ["queued behind"]);
    }

    #[test]
    fn a_barrier_waits_for_earlier_work_and_holds_back_later_work() {
        let mut lanes = Lanes::default();
        lanes.admit(page("a"), None, "a1");
        lanes.admit(Lane::Independent, None, "free");
        lanes.admit(Lane::Barrier, None, "close");
        lanes.admit(page("b"), None, "b1");
        lanes.admit(Lane::Independent, None, "late");
        let first = lanes.ready();
        assert_eq!(names(first.clone()), ["a1", "free"]);
        lanes.finish(ticket(&first, "free"));
        assert!(lanes.ready().is_empty());
        lanes.finish(ticket(&first, "a1"));
        let barrier = lanes.ready();
        assert_eq!(names(barrier.clone()), ["close"]);
        assert!(lanes.ready().is_empty());
        lanes.finish(ticket(&barrier, "close"));
        assert_eq!(names(lanes.ready()), ["b1", "late"]);
    }

    #[test]
    fn a_barrier_does_not_overtake_work_queued_before_it() {
        let mut lanes = Lanes::default();
        lanes.admit(page("a"), None, "a1");
        lanes.admit(page("a"), None, "a2");
        lanes.admit(Lane::Barrier, None, "launch");
        let first = lanes.ready();
        assert_eq!(names(first.clone()), ["a1"]);
        lanes.finish(ticket(&first, "a1"));
        let second = lanes.ready();
        assert_eq!(names(second.clone()), ["a2"]);
        lanes.finish(ticket(&second, "a2"));
        assert_eq!(names(lanes.ready()), ["launch"]);
    }

    #[test]
    fn a_control_follows_only_the_admission_of_its_own_operation() {
        let mut lanes = Lanes::default();
        lanes.admit(page("a"), None, "blocked read");
        lanes.admit(page("a"), Some("op".into()), "admission");
        lanes.admit_after_operation(Some("op".into()), "get");
        lanes.admit_after_operation(Some("op".into()), "cancel");
        lanes.admit_after_operation(Some("unknown".into()), "unrelated get");
        lanes.admit_after_operation(None, "malformed get");
        let first = lanes.ready();
        assert_eq!(
            names(first.clone()),
            ["blocked read", "unrelated get", "malformed get"]
        );
        lanes.finish(ticket(&first, "blocked read"));
        let admission = lanes.ready();
        assert_eq!(names(admission.clone()), ["admission"]);
        assert!(lanes.ready().is_empty());
        lanes.finish(ticket(&admission, "admission"));
        let get = lanes.ready();
        assert_eq!(names(get.clone()), ["get"]);
        lanes.finish(ticket(&get, "get"));
        assert_eq!(names(lanes.ready()), ["cancel"]);
    }

    #[test]
    fn a_control_for_a_running_admission_waits_for_it_to_finish() {
        let mut lanes = Lanes::default();
        lanes.admit(page("a"), Some("op".into()), "admission");
        let admission = lanes.ready();
        lanes.admit_after_operation(Some("op".into()), "get");
        assert!(lanes.ready().is_empty());
        lanes.finish(ticket(&admission, "admission"));
        assert_eq!(names(lanes.ready()), ["get"]);
        lanes.admit_after_operation(Some("op".into()), "late get");
        assert!(lanes.ready().is_empty());
    }
}
