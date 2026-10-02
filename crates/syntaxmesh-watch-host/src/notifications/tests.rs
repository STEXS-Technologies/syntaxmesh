use super::*;

#[test]
fn saturation_is_bounded_and_errors_remain_sticky() -> Result<(), Box<dyn std::error::Error>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let degraded = AtomicBool::new(false);
    for _ in 0..10_000 {
        enqueue(Ok(Event::new(EventKind::Any)), &[], &sender, &degraded);
    }
    enqueue(
        Err(notify::Error::generic("fixture backend failure")),
        &[],
        &sender,
        &degraded,
    );
    receiver.try_recv()?;
    if receiver.try_recv().is_ok() || !degraded.load(Ordering::Acquire) {
        return Err("notification queue grew or backend error disappeared".into());
    }
    enqueue(Ok(Event::new(EventKind::Any)), &[], &sender, &degraded);
    receiver.try_recv()?;
    if !degraded.load(Ordering::Acquire) {
        return Err("backend error was cleared".into());
    }
    Ok(())
}

#[test]
fn exclusions_access_and_rescan_flags_preserve_invalidation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let excluded = fixture.path().join("generated");
    let (sender, receiver) = mpsc::sync_channel(1);
    let degraded = AtomicBool::new(false);
    let event = Event::new(EventKind::Any).add_path(excluded.join("graph.db"));
    enqueue(
        Ok(event.clone()),
        std::slice::from_ref(&excluded),
        &sender,
        &degraded,
    );
    enqueue(
        Ok(Event::new(EventKind::Access(
            notify::event::AccessKind::Any,
        ))),
        &[],
        &sender,
        &degraded,
    );
    if receiver.try_recv().is_ok() {
        return Err("excluded/read event invalidated inventory".into());
    }
    enqueue(
        Ok(event.set_flag(notify::event::Flag::Rescan)),
        &[excluded],
        &sender,
        &degraded,
    );
    receiver.try_recv()?;
    Ok(())
}

#[test]
fn native_watcher_reports_a_file_write() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let notifications = FilesystemNotifications::open(fixture.path(), vec![])?;
    if notifications.receive(Duration::ZERO)?.is_some() {
        return Err("unexpected initial event".into());
    }
    std::fs::write(fixture.path().join("lib.rs"), "pub fn watched() {}")?;
    let notice = notifications
        .receive(Duration::from_secs(5))?
        .ok_or("native watcher missed file write")?;
    if notice.degraded {
        return Err("native watcher reported backend failure".into());
    }
    Ok(())
}
