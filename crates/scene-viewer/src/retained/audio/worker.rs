use super::{
    State,
    ring::{BLOCK, Ring},
};
use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};

pub(super) fn start(state: Arc<Mutex<State>>, ring: Arc<Ring>) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("short-clip-mixer".into())
        .spawn(move || {
            let mut output = [[0.0; 2]; BLOCK];
            while ring.running.load(Ordering::Acquire) {
                if ring.free() >= BLOCK {
                    let mut state = state.lock().expect("audio mixer poisoned");
                    state.mixer.render(&mut output);
                    let end = ring.push(&output);
                    while let Some(voice) = state.mixer.take_finished() {
                        state.pending.push_back((voice, end));
                    }
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        })
}
