use obscura_dom::DomTree;

use crate::trace::TraceEvent;
use crate::RuntimeOptions;

pub(crate) struct NativeState {
    pub(crate) options: RuntimeOptions,
    pub(crate) dom: DomTree,
    pub(crate) title: String,
    pub(crate) cookies: Vec<(String, String)>,
    pub(crate) next_timer_id: u32,
    pub(crate) timers: Vec<TimerTask>,
    pub(crate) trace: Vec<TraceEvent>,
}

pub(crate) struct TimerTask {
    pub(crate) id: u32,
    pub(crate) callback: v8::Global<v8::Function>,
    pub(crate) repeat: bool,
}
