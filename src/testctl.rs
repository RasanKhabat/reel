//! Test mode: takes commands over a distributed notification and writes the strip state as
//! JSON after each one has settled (see test_mode.rs).

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSOperationQueue, NSString};
use serde_json::json;
use std::cell::RefCell;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

thread_local! {
    static TOKEN: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> = const { RefCell::new(None) };
}

fn field(note: &NSNotification, key: &str) -> Option<String> {
    let info = note.userInfo()?;
    let v = info.objectForKey(&NSString::from_str(key))?;
    v.downcast::<NSString>().ok().map(|s| s.to_string())
}

pub fn start() {
    if !crate::test_mode::is_on() {
        return;
    }
    let block = RcBlock::new(|note: NonNull<NSNotification>| {
        let note = unsafe { note.as_ref() };
        let command = field(note, "command").unwrap_or_default();
        let id = field(note, "id").unwrap_or_else(|| "0".into());
        run(command, id);
    });
    let token = unsafe {
        NSDistributedNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(&NSString::from_str(crate::test_mode::COMMAND_NOTIFICATION)),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    TOKEN.with(|t| *t.borrow_mut() = Some(token));
    crate::log::write(&format!("test mode: managing pid {} only", crate::test_mode::pid().unwrap_or(0)));
    write_state("ready");
}

fn run(command: String, id: String) {
    crate::log::write(&format!("test command: {command} [{id}]"));
    match command.as_str() {
        "start" => crate::set_tiling(true),
        "stop" => crate::set_tiling(false),
        "rescan" => crate::with_desk(|d| d.rescan_all()),
        "quit" => {
            write_state(&id);
            crate::quit();
            return;
        }
        "dump" => {}
        other => match crate::shortcuts::Command::from_raw(other) {
            Some(c) => crate::with_desk(move |d| {
                d.perform(c);
            }),
            None => crate::log::write(&format!("unknown test command {other}")),
        },
    }
    // A slow app (a cold first activation can stall for seconds) keeps moves queued;
    // wait for it rather than reporting a state that is about to change.
    wait_for_quiet(id, Instant::now() + Duration::from_secs(8));
}

/// Reports once every animation has settled and every queued move has landed.
fn wait_for_quiet(id: String, deadline: Instant) {
    // Also wait out the post-focus check (it runs while the focus guard is armed).
    let busy = crate::with_desk_sync(|d| d.any_animating()).unwrap_or(true)
        || !crate::mover::Mover::is_idle()
        || crate::controller::focus_guard_armed()
        || crate::controller::pending_checks() > 0;
    if busy && Instant::now() < deadline {
        crate::ffi::main_after(0.05, move || wait_for_quiet(id, deadline));
        return;
    }
    if busy {
        crate::log::write(&format!(
            "  test: still busy at deadline (animating {:?}, mover idle {}, guard {}, checks {})",
            crate::with_desk_sync(|d| d.any_animating()),
            crate::mover::Mover::is_idle(),
            crate::controller::focus_guard_armed(),
            crate::controller::pending_checks()
        ));
    }
    crate::ffi::main_after(0.15, move || write_state(&id));
}

fn write_state(id: &str) {
    let Some(dir) = crate::test_mode::directory() else { return };
    let state = crate::with_desk_sync(|d| {
        let strips: Vec<_> = d
            .strips
            .iter()
            .map(|s| {
                let u = s.usable();
                json!({
                    "usable": [u.x, u.y, u.w, u.h],
                    "gap": s.strip.gap,
                    "focus": s.strip.focus,
                    "offset": s.strip.offset,
                    "columns": s.strip.columns.iter().map(|c| json!({
                        "share": c.share,
                        "row": c.row,
                        "tabbed": c.tabbed,
                        "width": s.strip.width(c),
                        "windows": c.windows.iter().map(|id| s.wins.get(id).map(|w| w.title()).unwrap_or_default()).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({ "id": id, "tiling": d.is_tiling, "strips": strips, "pid": std::process::id() })
    });
    let Some(state) = state else {
        let id = id.to_string();
        crate::ffi::main_after(0.05, move || write_state(&id));
        return;
    };
    let path = dir.join(format!("reel-{id}.json"));
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(&state).unwrap_or_default()).and_then(|_| std::fs::rename(&tmp, &path)).is_err() {
        crate::log::write("test state write failed");
    }
}
