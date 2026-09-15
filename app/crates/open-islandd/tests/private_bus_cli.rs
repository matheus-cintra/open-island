#![cfg(target_os = "linux")]
#[path = "support/notification_bus.rs"]
mod notification_bus;
use notification_bus::TestBus;

#[test]
fn private_bus_guard_cleans_after_normal_exit_and_panic() {
    for panic in [false, true] {
        let bus = TestBus::start().expect("private dbus-daemon prerequisite");
        let pid = bus.pid();
        let socket = bus.socket();
        let address = bus.address();
        let connection = zbus::blocking::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .unwrap();
        let proxy = zbus::blocking::fdo::DBusProxy::new(&connection).unwrap();
        assert!(!proxy.list_names().unwrap().is_empty());
        drop(proxy);
        drop(connection);
        let result = std::panic::catch_unwind(move || {
            let _owned = bus;
            assert!(!panic, "exercise unwinding");
        });
        assert_eq!(result.is_err(), panic);
        assert!(!socket.exists());
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}
