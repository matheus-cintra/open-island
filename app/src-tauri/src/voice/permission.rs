use super::{native_capture::PermissionGate, pcm::Error};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct NativePermission<'a> {
    pub cancelled: &'a AtomicBool,
}

impl PermissionGate for NativePermission<'_> {
    fn authorize(&self) -> Result<(), Error> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        #[cfg(feature = "qa-harness")]
        {
            Err(Error::Unavailable)
        }
        #[cfg(all(not(feature = "qa-harness"), target_os = "macos"))]
        {
            extern "C" {
                fn oi_microphone_authorization() -> i32;
                fn oi_request_microphone();
            }
            authorize_with(
                self.cancelled,
                || unsafe { oi_microphone_authorization() },
                || unsafe { oi_request_microphone() },
                || std::thread::sleep(std::time::Duration::from_millis(10)),
            )
        }
        #[cfg(all(not(feature = "qa-harness"), not(target_os = "macos")))]
        {
            // Linux has no application-wide microphone consent dialog here.
            // CPAL/ALSA enforces device access and maps PermissionDenied.
            Ok(())
        }
    }
}

#[cfg(any(test, all(not(feature = "qa-harness"), target_os = "macos")))]
fn authorize_with(
    cancelled: &AtomicBool,
    mut status: impl FnMut() -> i32,
    request: impl FnOnce(),
    mut wait: impl FnMut(),
) -> Result<(), Error> {
    let mut request = Some(request);
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        match status() {
            0 => {
                if let Some(request) = request.take() {
                    request();
                }
                wait();
            }
            1 | 2 => return Err(Error::Permission),
            3 => return Ok(()),
            _ => return Err(Error::Unavailable),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn denied_restricted_authorized_and_unknown_do_not_prompt() {
        for (status, expected) in [
            (1, Err(Error::Permission)),
            (2, Err(Error::Permission)),
            (3, Ok(())),
            (4, Err(Error::Unavailable)),
        ] {
            assert_eq!(
                authorize_with(
                    &AtomicBool::new(false),
                    || status,
                    || panic!("unexpected request"),
                    || panic!("unexpected wait")
                ),
                expected
            );
        }
    }
    #[test]
    fn permission_requests_once_and_wait_can_be_cancelled() {
        let cancel = AtomicBool::new(false);
        let requests = Cell::new(0);
        let waits = Cell::new(0);
        let result = authorize_with(
            &cancel,
            || 0,
            || requests.set(requests.get() + 1),
            || {
                waits.set(waits.get() + 1);
                if waits.get() == 3 {
                    cancel.store(true, Ordering::Release);
                }
            },
        );
        assert_eq!(result, Err(Error::Cancelled));
        assert_eq!(requests.get(), 1);
        assert_eq!(waits.get(), 3);
        let statuses = Cell::new(0);
        assert_eq!(
            authorize_with(
                &AtomicBool::new(false),
                || {
                    statuses.set(statuses.get() + 1);
                    if statuses.get() == 1 {
                        0
                    } else {
                        3
                    }
                },
                || {},
                || {}
            ),
            Ok(())
        );
    }
}
