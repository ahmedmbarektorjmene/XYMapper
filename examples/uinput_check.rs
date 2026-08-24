fn main() {
    use uinput::event::absolute::{Absolute, Position};
    use uinput::event::controller::{Controller, GamePad};
    use uinput::event::Event;
    let name = "XXMapper Debug Pad";
    let mut b = uinput::default().expect("default");
    b = b.name(name).expect("name");
    b = b.event(Event::Controller(Controller::GamePad(GamePad::South))).expect("ev");
    b = b.event(Event::Absolute(Absolute::Position(Position::X))).expect("abs").min(-32767).max(32767).flat(0).fuzz(0);
    let mut dev = b.create().expect("create");
    dev.send(Event::Controller(Controller::GamePad(GamePad::South)), 1).expect("send");
    dev.synchronize().expect("sync");
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!("--- /proc/bus/input/devices (tail) ---");
    let procs = std::fs::read_to_string("/proc/bus/input/devices").unwrap();
    for line in procs.lines().rev().take(20) { println!("{line}"); }
    std::thread::sleep(std::time::Duration::from_secs(3));
}
