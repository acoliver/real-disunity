use crate::lifecycle::EnteredState;
use crate::{
    Clock, ComponentHandle, DestroyWhen, InputPhase, LifecycleWork, MutationId, ScriptMessage,
    TimeError, TimeSpan, WaitCondition, World, WorldError,
};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptFailure {
    pub message: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallbackTicket {
    world: u64,
    sequence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetCallbackWork {
    pub ticket: CallbackTicket,
    pub identity: u64,
    pub message: ScriptMessage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallbackWork {
    pub ticket: CallbackTicket,
    pub script: ComponentHandle,
    pub message: ScriptMessage,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallbackOutcome {
    Returned,
    Yielded(WaitCondition),
    Failed(ScriptFailure),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSummary {
    pub frame: u64,
    pub fixed_steps: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Advance {
    Callback(CallbackWork),
    AssetCallback(AssetCallbackWork),
    LifecycleComplete,
    Complete(FrameSummary),
}
/// Where in a frame coroutine waits are checked. An end-of-frame wait is due only in `End`,
/// which follows `LateUpdate`; a coroutine that yields it from `End` waits for the next frame's.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoroutinePhase {
    Fixed,
    Update,
    End,
}
#[derive(Clone, Copy)]
pub(crate) enum Task {
    Work(LifecycleWork),
    Asset(u64, ScriptMessage, bool),
    Teardown(LifecycleWork),
    Phase(ScriptMessage),
    Fixed(crate::clock::FixedPhase),
    Physics,
    Contacts,
    Dynamic,
    Coroutines(CoroutinePhase),
    /// Unity's delayed tasks after `Update` coroutines: work posted to the main thread's
    /// synchronization context runs here.
    DelayedTasks,
    /// After `LateUpdate` and before the frame is drawn, the same host identity gets
    /// `LateUpdate`, where Unity raises the canvas events that build UI for the frame.
    WillRender,
    Cleanup,
}
#[derive(Clone, Copy)]
enum Suspended {
    Script(CallbackWork),
    Asset(AssetCallbackWork),
}
impl Suspended {
    fn ticket(self) -> CallbackTicket {
        match self {
            Self::Script(work) => work.ticket,
            Self::Asset(work) => work.ticket,
        }
    }
}
#[derive(Clone, Default)]
struct Scope {
    lifecycle: bool,
    tasks: VecDeque<Task>,
    callback: Option<Suspended>,
}
#[derive(Clone, Default)]
pub(crate) struct Scheduler {
    scopes: Vec<Scope>,
    /// Set at the start of a frame: the next look at new scripts revisits every script.
    rescan_scripts: bool,
    sequence: u64,
    fixed_steps: u32,
    frame_delta: TimeSpan,
    frame_unscaled_delta: TimeSpan,
    failure: Option<ScriptFailure>,
    faulted: bool,
    terminal_failure: Option<ScriptFailure>,
    /// The host identity that receives the delayed-tasks callback each frame.
    pub(crate) delayed_tasks: Option<u64>,
}
impl World {
    pub fn script_retained(&self, script: ComponentHandle) -> bool {
        self.component(script).is_ok()
            || self.lifecycle.cleanup.iter().any(|work| work.script == script)
            || self.scheduler.scopes.iter().any(|scope| {
                matches!(scope.callback, Some(Suspended::Script(work)) if work.script == script)
                    || scope.tasks.iter().any(|task| matches!(task, Task::Work(work) | Task::Teardown(work) if work.script == script))
            })
    }
    pub fn callback_pending(&self, ticket: CallbackTicket) -> bool {
        self.scheduler
            .scopes
            .iter()
            .any(|scope| scope.callback.is_some_and(|work| work.ticket() == ticket))
    }
    pub fn terminal_failure(&self) -> Option<&ScriptFailure> {
        self.scheduler.terminal_failure.as_ref()
    }
    pub fn clock(&self) -> &Clock {
        &self.clock
    }
    pub(crate) fn ensure_allocation_allowed(&self) -> Result<(), WorldError> {
        if self.scheduler.failure.is_some() || self.scheduler.faulted {
            return Err(WorldError::Faulted);
        }
        Ok(())
    }
    pub(crate) fn ensure_no_advance(&self) -> Result<(), WorldError> {
        self.require_idle()
    }
    fn require_idle(&self) -> Result<(), WorldError> {
        if self.scheduler.faulted {
            return Err(WorldError::Faulted);
        }
        if !self.scheduler.scopes.is_empty() {
            return Err(WorldError::AdvanceInProgress);
        }
        Ok(())
    }
    pub fn set_time_scale(&mut self, scale: TimeSpan) -> Result<(), WorldError> {
        self.ensure_allocation_allowed()?;
        if scale > TimeSpan::new(100, 1)? {
            return Err(TimeError::Configuration.into());
        }
        self.clock.time_scale = scale;
        Ok(())
    }
    /// Sets the longest frame the clock accepts (`Time.maximumDeltaTime`); zero is refused.
    pub fn set_max_delta(&mut self, limit: TimeSpan) -> Result<(), WorldError> {
        self.ensure_allocation_allowed()?;
        if limit == TimeSpan::ZERO {
            return Err(TimeError::Configuration.into());
        }
        self.clock.max_delta = limit;
        Ok(())
    }
    pub fn configure_fixed_step(
        &mut self,
        step: TimeSpan,
        max_steps: u32,
    ) -> Result<(), WorldError> {
        self.require_idle()?;
        if step == TimeSpan::ZERO
            || step > TimeSpan::ONE
            || !(1..=1024).contains(&max_steps)
            || self.clock.frame != 0
        {
            return Err(TimeError::Configuration.into());
        }
        self.clock.fixed_step = step;
        self.clock.max_steps = max_steps;
        Ok(())
    }
    /// Names the host identity that receives an `Update` asset callback once a frame, after
    /// `Update` coroutines and before `LateUpdate`, where Unity runs the main thread's
    /// delayed tasks, and a `LateUpdate` asset callback once a frame after the scripts'
    /// `LateUpdate`, where the canvas events are raised; `None` stops both.
    pub fn set_delayed_tasks(&mut self, identity: Option<u64>) -> Result<(), WorldError> {
        self.require_idle()?;
        // Callbacks are routed by identity, so the phase cannot share one with a script asset.
        if identity.is_some_and(|id| id == 0 || self.script_asset_alive(id)) {
            return Err(WorldError::InvalidObject);
        }
        self.scheduler.delayed_tasks = identity;
        Ok(())
    }
    pub fn advance(&mut self, elapsed: TimeSpan) -> Result<Advance, WorldError> {
        self.require_idle()?;
        let (clock, phases) = self.clock.plan(elapsed)?;
        self.scheduler.fixed_steps = u32::try_from(phases.len()).expect("bounded steps");
        self.scheduler.frame_delta = clock.delta;
        self.scheduler.frame_unscaled_delta = clock.unscaled_delta;
        self.scheduler.rescan_scripts = true;
        self.clock = clock;
        self.expire_destruction()?;
        let mut tasks = VecDeque::from([Task::Phase(ScriptMessage::Start)]);
        for phase in phases {
            tasks.push_back(Task::Fixed(phase));
        }
        tasks.extend([
            Task::Dynamic,
            Task::Phase(ScriptMessage::Update),
            Task::Coroutines(CoroutinePhase::Update),
            Task::DelayedTasks,
            Task::Phase(ScriptMessage::LateUpdate),
            Task::WillRender,
            Task::Coroutines(CoroutinePhase::End),
            Task::Cleanup,
        ]);
        self.scheduler.scopes.push(Scope {
            tasks,
            callback: None,
            lifecycle: false,
        });
        self.initialize_scripts()?;
        self.poll_scheduler()
    }
    /// Opens a child scope without releasing the suspended parent's ticket.
    /// The host drains it through resume until `LifecycleComplete` before returning the clone.
    pub fn next_lifecycle(&mut self, mutation: MutationId) -> Result<Advance, WorldError> {
        self.check_lifecycle_scope()?;
        self.open_lifecycle(mutation)
    }
    /// Opens a child scope that runs the `OnDisable` and `OnDestroy` messages queued by destruction so
    /// far, instead of leaving them for the frame's cleanup. Like `next_lifecycle`, the host drains it
    /// through resume until `LifecycleComplete`; each message runs once.
    pub fn flush_cleanup(&mut self) -> Result<Advance, WorldError> {
        self.check_lifecycle_scope()?;
        let mutation = self.reserve_mutation()?;
        let work = self.take_cleanup();
        self.lifecycle.mutations.insert(mutation.sequence, work);
        self.open_lifecycle(mutation)
    }
    /// A lifecycle scope may open at top level or inside a script callback.
    pub(crate) fn check_lifecycle_scope(&self) -> Result<(), WorldError> {
        if self.scheduler.faulted {
            return Err(WorldError::Faulted);
        }
        if self
            .scheduler
            .scopes
            .last()
            .is_some_and(|scope| scope.callback.is_none())
        {
            return Err(WorldError::WrongCallback);
        }
        Ok(())
    }
    pub(crate) fn open_lifecycle(&mut self, mutation: MutationId) -> Result<Advance, WorldError> {
        if self.scheduler.scopes.is_empty() {
            self.lifecycle.added.clear();
            self.scheduler.rescan_scripts = false;
        }
        let tasks = self
            .take_lifecycle_work(mutation)?
            .into_iter()
            .map(Task::Work)
            .collect();
        self.scheduler.scopes.push(Scope {
            tasks,
            callback: None,
            lifecycle: true,
        });
        self.poll_scheduler()
    }
    pub fn resume(
        &mut self,
        ticket: CallbackTicket,
        outcome: CallbackOutcome,
    ) -> Result<Advance, WorldError> {
        let callback = self
            .scheduler
            .scopes
            .last()
            .and_then(|scope| scope.callback)
            .ok_or(WorldError::WrongCallback)?;
        if ticket != callback.ticket() {
            return Err(WorldError::WrongCallback);
        }
        if matches!(outcome, CallbackOutcome::Yielded(_))
            && !matches!(
                callback,
                Suspended::Script(CallbackWork {
                    message: ScriptMessage::Coroutine(_),
                    ..
                })
            )
        {
            return Err(WorldError::WrongCallback);
        }
        if let CallbackOutcome::Yielded(wait) = outcome {
            let Suspended::Script(CallbackWork {
                script,
                message: ScriptMessage::Coroutine(id),
                ..
            }) = callback
            else {
                unreachable!()
            };
            if self.coroutines.entries.contains_key(&id) {
                let entry = self.coroutine_wait(script, wait)?;
                self.coroutines.entries.insert(id, entry);
            }
        } else if let Suspended::Script(CallbackWork {
            message: ScriptMessage::Coroutine(id),
            ..
        }) = callback
        {
            if matches!(outcome, CallbackOutcome::Failed(_)) {
                self.coroutines.cancel(id);
            } else {
                self.coroutines.entries.remove(&id);
            }
        }
        self.scheduler
            .scopes
            .last_mut()
            .expect("checked scope")
            .callback = None;
        if self.scheduler.failure.is_none() {
            self.initialize_scripts()?;
        }
        if let CallbackOutcome::Failed(failure) = outcome {
            self.fail_callback(failure)?;
        }
        self.poll_scheduler()
    }
    /// Runs the lifecycle of objects whose scripts were registered since the last look, or of
    /// every scripted object at the start of a frame. Only the new registrations are read
    /// otherwise, so a callback's resume does not cost a pass over every script.
    fn initialize_scripts(&mut self) -> Result<(), WorldError> {
        let added = std::mem::take(&mut self.lifecycle.added);
        let candidates: Vec<_> = if std::mem::take(&mut self.scheduler.rescan_scripts) {
            self.lifecycle.scripts.keys().copied().collect()
        } else {
            added
        };
        let nodes: BTreeSet<_> = candidates
            .into_iter()
            .filter(|script| self.lifecycle.scripts.contains_key(script))
            .filter_map(|script| self.component(script).ok().map(|c| c.object))
            .filter(|&object| !self.is_unpublished(object))
            .collect();
        if !nodes.is_empty() {
            let mutation = self.reserve_mutation()?;
            self.refresh_lifecycle(&nodes.into_iter().collect::<Vec<_>>(), mutation);
        }
        Ok(())
    }
    fn enqueue_front(&mut self, tasks: impl IntoIterator<Item = Task>) {
        let tasks: Vec<_> = tasks.into_iter().collect();
        let scope = self.scheduler.scopes.last_mut().expect("active scope");
        for task in tasks.into_iter().rev() {
            scope.tasks.push_front(task);
        }
    }
    fn pending_lifecycle(&mut self) {
        let ready: Vec<_> = self
            .lifecycle
            .mutations
            .keys()
            .copied()
            .filter(|&id| !self.lifecycle.unpublished.contains_key(id))
            .collect();
        let batches: Vec<_> = ready
            .into_iter()
            .flat_map(|id| {
                self.lifecycle
                    .mutations
                    .remove(&id)
                    .expect("selected mutation")
            })
            .collect();
        self.enqueue_front(batches.into_iter().map(Task::Work));
    }
    fn eligible(&self, script: ComponentHandle) -> bool {
        self.script_ready(script).unwrap_or(false)
    }
    fn phase_tasks(&mut self, message: ScriptMessage) -> Vec<Task> {
        let scripts: Vec<_> = self.lifecycle.scripts.keys().copied().collect();
        let mut tasks = Vec::new();
        for script in scripts {
            if !self.eligible(script) {
                continue;
            }
            let state = self
                .lifecycle
                .scripts
                .get_mut(&script)
                .expect("registered script");
            if state.start == crate::lifecycle::StartState::Pending {
                tasks.push(Task::Work(LifecycleWork {
                    script,
                    message: ScriptMessage::Start,
                }));
            }
            if message != ScriptMessage::Start {
                tasks.push(Task::Work(LifecycleWork { script, message }));
            }
        }
        tasks
    }
    fn due_coroutines(&mut self, phase: CoroutinePhase) -> Vec<Task> {
        let ids: Vec<_> = self.coroutines.entries.keys().copied().collect();
        let mut tasks = Vec::new();
        for id in ids {
            let entry = &self.coroutines.entries[&id];
            if !self.coroutine_eligible(entry.script) {
                self.coroutines.cancel(id);
                continue;
            }
            let fixed = phase == CoroutinePhase::Fixed;
            let update = phase == CoroutinePhase::Update;
            let due = match entry.wait {
                WaitCondition::FixedUpdate => fixed,
                WaitCondition::EndOfFrame => phase == CoroutinePhase::End,
                WaitCondition::NextFrame => update && entry.frame < self.clock.frame,
                WaitCondition::Scaled(_) => {
                    update
                        && self.clock.delta != TimeSpan::ZERO
                        && entry.deadline <= self.clock.time
                        && entry.frame < self.clock.frame
                }
                WaitCondition::Unscaled(_) => {
                    update
                        && entry.deadline <= self.clock.unscaled_time
                        && entry.frame < self.clock.frame
                }
            };
            if due {
                tasks.push(Task::Work(LifecycleWork {
                    script: entry.script,
                    message: ScriptMessage::Coroutine(id),
                }));
            }
        }
        tasks
    }
    fn fail_callback(&mut self, failure: ScriptFailure) -> Result<(), WorldError> {
        if self.scheduler.failure.is_some() {
            return Ok(());
        }
        self.scheduler.terminal_failure = Some(failure.clone());
        self.scheduler.failure = Some(failure);
        self.coroutines.cancel_all();
        self.fail_script_assets();
        let mut teardown: VecDeque<_> = self
            .scheduler
            .scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.tasks.iter().copied())
            .filter(|task| matches!(task, Task::Teardown(_) | Task::Asset(_, _, true)))
            .collect();
        teardown.push_back(Task::Cleanup);
        self.scheduler.scopes.clear();
        self.scheduler.scopes.push(Scope {
            tasks: teardown,
            callback: None,
            lifecycle: false,
        });
        self.lifecycle.mutations.clear();
        let objects: Vec<_> = self
            .snapshot()
            .objects
            .into_iter()
            .map(|o| o.handle)
            .collect();
        for object in objects {
            self.destroy(object, DestroyWhen::EndOfFrame)?;
        }
        Ok(())
    }
    fn enter_activation(&mut self, mut work: LifecycleWork) -> Option<LifecycleWork> {
        if work.message == ScriptMessage::OnEnable {
            let state = &self.lifecycle.scripts[&work.script];
            if state.entered == EnteredState::Enabled {
                return None;
            }
            if state.entered == EnteredState::Dormant {
                self.enqueue_front([Task::Work(work)]);
                work.message = ScriptMessage::Awake;
            }
        }
        if work.message == ScriptMessage::Awake {
            let active = self
                .component(work.script)
                .is_ok_and(|c| self.active_in_hierarchy(c.object).expect("live hierarchy"));
            let state = self
                .lifecycle
                .scripts
                .get_mut(&work.script)
                .expect("live script");
            if state.entered != EnteredState::Dormant {
                return None;
            }
            if !active {
                state.awake = false;
                return None;
            }
            state.entered = EnteredState::Awake;
        } else if let Some(state) = self.lifecycle.scripts.get_mut(&work.script) {
            match work.message {
                ScriptMessage::OnEnable => state.entered = EnteredState::Enabled,
                ScriptMessage::OnDisable if state.entered == EnteredState::Enabled => {
                    state.entered = EnteredState::Awake;
                }
                _ => {}
            }
        }
        Some(work)
    }
    fn dispatch(&mut self, work: LifecycleWork) -> Result<Option<Advance>, WorldError> {
        let cleanup = matches!(
            work.message,
            ScriptMessage::OnDisable | ScriptMessage::OnDestroy
        );
        if !cleanup
            && (self.lifetime.components.contains(&work.script)
                || self.component(work.script).is_err())
        {
            return Ok(None);
        }
        if matches!(
            work.message,
            ScriptMessage::OnEnable
                | ScriptMessage::Start
                | ScriptMessage::FixedUpdate
                | ScriptMessage::Update
                | ScriptMessage::LateUpdate
                | ScriptMessage::Contact2d(_)
        ) && !self.eligible(work.script)
        {
            return Ok(None);
        }
        let Some(work) = self.enter_activation(work) else {
            return Ok(None);
        };
        if let ScriptMessage::Contact2d(id) = work.message
            && !self
                .contacts()
                .get(id.index)
                .is_some_and(|event| self.contact_eligible(event))
        {
            return Ok(None);
        }
        if let ScriptMessage::Coroutine(id) = work.message
            && (!self.coroutines.entries.contains_key(&id) || !self.coroutine_eligible(work.script))
        {
            return Ok(None);
        }
        if work.message == ScriptMessage::Start {
            self.lifecycle
                .scripts
                .get_mut(&work.script)
                .expect("ready script")
                .start = crate::lifecycle::StartState::Started;
        }
        self.issue_callback(work)
    }
    fn issue_callback(&mut self, work: LifecycleWork) -> Result<Option<Advance>, WorldError> {
        self.scheduler.sequence = self
            .scheduler
            .sequence
            .checked_add(1)
            .ok_or(TimeError::Overflow)?;
        let callback = CallbackWork {
            ticket: CallbackTicket {
                world: self.identity,
                sequence: self.scheduler.sequence,
            },
            script: work.script,
            message: work.message,
        };
        self.scheduler
            .scopes
            .last_mut()
            .expect("active scope")
            .callback = Some(Suspended::Script(callback));
        Ok(Some(Advance::Callback(callback)))
    }
    pub(crate) fn open_asset_scope(&mut self, identity: u64) -> Result<Advance, WorldError> {
        if self
            .scheduler
            .scopes
            .last()
            .is_some_and(|scope| scope.callback.is_none())
        {
            return Err(WorldError::WrongCallback);
        }
        self.scheduler.scopes.push(Scope {
            lifecycle: true,
            tasks: VecDeque::from([
                Task::Asset(identity, ScriptMessage::Awake, false),
                Task::Asset(identity, ScriptMessage::OnEnable, false),
            ]),
            callback: None,
        });
        self.poll_scheduler()
    }
    fn issue_asset_callback(
        &mut self,
        identity: u64,
        message: ScriptMessage,
    ) -> Result<Option<Advance>, WorldError> {
        if !self.enter_asset(identity, message) {
            return Ok(None);
        }
        self.issue_identity_callback(identity, message)
    }
    /// Suspends the scheduler on a callback to a host identity that is not a script.
    fn issue_identity_callback(
        &mut self,
        identity: u64,
        message: ScriptMessage,
    ) -> Result<Option<Advance>, WorldError> {
        self.scheduler.sequence = self
            .scheduler
            .sequence
            .checked_add(1)
            .ok_or(TimeError::Overflow)?;
        let callback = AssetCallbackWork {
            ticket: CallbackTicket {
                world: self.identity,
                sequence: self.scheduler.sequence,
            },
            identity,
            message,
        };
        self.scheduler
            .scopes
            .last_mut()
            .expect("active scope")
            .callback = Some(Suspended::Asset(callback));
        Ok(Some(Advance::AssetCallback(callback)))
    }
    fn poll_scheduler(&mut self) -> Result<Advance, WorldError> {
        loop {
            self.pending_lifecycle();
            let task = self
                .scheduler
                .scopes
                .last_mut()
                .expect("active scope")
                .tasks
                .pop_front();
            let Some(task) = task else {
                let scope = self.scheduler.scopes.pop().expect("active scope");
                if scope.lifecycle {
                    return Ok(Advance::LifecycleComplete);
                }
                self.finish_phase();
                if let Some(failure) = self.scheduler.failure.take() {
                    self.scheduler.faulted = true;
                    return Err(WorldError::CallbackFailed(failure));
                }
                return Ok(Advance::Complete(FrameSummary {
                    frame: self.clock.frame,
                    fixed_steps: self.scheduler.fixed_steps,
                }));
            };
            if let Some(work) = self.run_task(task)? {
                return Ok(work);
            }
        }
    }
    fn enqueue_cleanup(&mut self) {
        let mut work: Vec<_> = self
            .take_cleanup()
            .into_iter()
            .map(Task::Teardown)
            .collect();
        work.extend(
            self.asset_cleanup()
                .into_iter()
                .map(|(id, message)| Task::Asset(id, message, true)),
        );
        if !work.is_empty() {
            self.enqueue_front(work.into_iter().chain([Task::Cleanup]));
        }
    }
    fn run_task(&mut self, task: Task) -> Result<Option<Advance>, WorldError> {
        match task {
            Task::Work(work) | Task::Teardown(work) => return self.dispatch(work),
            Task::Phase(message) => {
                let tasks = self.phase_tasks(message);
                self.enqueue_front(tasks);
            }
            Task::Fixed(phase) => {
                self.clock.fixed_frame = phase.frame;
                self.clock.fixed_time = phase.time;
                self.clock.delta = self.clock.fixed_step;
                self.clock.unscaled_delta = phase.unscaled_delta;
                self.input.project(InputPhase::Fixed, phase.cutoff);
                self.enqueue_front([
                    Task::Phase(ScriptMessage::FixedUpdate),
                    Task::Physics,
                    Task::Contacts,
                    Task::Coroutines(CoroutinePhase::Fixed),
                ]);
            }
            Task::Physics => {
                #[allow(clippy::cast_possible_truncation)]
                let delta = self.clock.fixed_step.seconds() as f32;
                if let Err(error) = self.step_physics_inner(delta) {
                    self.scheduler.faulted = true;
                    self.scheduler.scopes.clear();
                    return Err(error);
                }
            }
            Task::Dynamic => {
                self.clock.delta = self.scheduler.frame_delta;
                self.clock.unscaled_delta = self.scheduler.frame_unscaled_delta;
                self.input
                    .project(InputPhase::Dynamic, self.clock.unscaled_time);
            }
            Task::Contacts => {
                let tasks = self.contact_tasks();
                self.enqueue_front(tasks);
            }
            Task::Coroutines(phase) => {
                let tasks = self.due_coroutines(phase);
                self.enqueue_front(tasks);
            }
            Task::Asset(identity, message, _) => {
                return self.issue_asset_callback(identity, message);
            }
            Task::DelayedTasks => {
                if let Some(identity) = self.scheduler.delayed_tasks {
                    return self.issue_identity_callback(identity, ScriptMessage::Update);
                }
            }
            Task::WillRender => {
                if let Some(identity) = self.scheduler.delayed_tasks {
                    return self.issue_identity_callback(identity, ScriptMessage::LateUpdate);
                }
            }
            Task::Cleanup => self.enqueue_cleanup(),
        }
        Ok(None)
    }
}
