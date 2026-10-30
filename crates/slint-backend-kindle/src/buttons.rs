//! Optional physical page-turn buttons on reviewed Kindle Oasis hardware.

use std::ffi::CString;

use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{Key, WindowEvent};

const EVENT_KEY: u16 = 1;
const KEY_PAGE_UP: u16 = 104;
const KEY_PAGE_DOWN: u16 = 109;
const GPIO_KEYS_NAME: &[u8] = b"gpio-keys";
const BUS_HOST: u16 = 0x0019;
const REVIEWED_VENDOR: u16 = 0x0001;
const REVIEWED_PRODUCT: u16 = 0x0001;
const REVIEWED_VERSION: u16 = 0x0100;
const EVIOCGRAB: libc::c_ulong = 0x4004_4590;

#[repr(C)]
struct InputEvent {
    timestamp_seconds: u32,
    timestamp_microseconds: u32,
    kind: u16,
    code: u16,
    value: i32,
}

#[repr(C)]
#[derive(Default)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PageButton {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ButtonState {
    Pressed,
    Released,
}

fn ioctl_read_request(number: libc::c_ulong, size: usize) -> libc::c_ulong {
    (2 << 30)
        | (libc::c_ulong::try_from(size).expect("fixed ioctl buffer length fits c_ulong") << 16)
        | ((b'E' as libc::c_ulong) << 8)
        | number
}

#[cfg(target_env = "musl")]
unsafe fn input_ioctl(
    file_descriptor: libc::c_int,
    request: libc::c_ulong,
    buffer: *mut u8,
) -> libc::c_int {
    let request_bits = u32::try_from(request).expect("Linux input ioctl request fits 32 bits");
    // SAFETY: the caller guarantees that buffer is writable for the size encoded in request.
    unsafe {
        libc::ioctl(
            file_descriptor,
            libc::c_int::from_ne_bytes(request_bits.to_ne_bytes()),
            buffer,
        )
    }
}

#[cfg(not(target_env = "musl"))]
unsafe fn input_ioctl(
    file_descriptor: libc::c_int,
    request: libc::c_ulong,
    buffer: *mut u8,
) -> libc::c_int {
    // SAFETY: the caller guarantees that buffer is writable for the size encoded in request.
    unsafe { libc::ioctl(file_descriptor, request, buffer) }
}

fn decoded_page_button(kind: u16, code: u16, value: i32) -> Option<(PageButton, ButtonState)> {
    if kind != EVENT_KEY {
        return None;
    }
    let button = match code {
        KEY_PAGE_UP => PageButton::Previous,
        KEY_PAGE_DOWN => PageButton::Next,
        _ => return None,
    };
    let state = match value {
        0 => ButtonState::Released,
        1 => ButtonState::Pressed,
        // A held bezel key should turn one page, not race through the document.
        2 => return None,
        _ => return None,
    };
    Some((button, state))
}

fn bit_is_set(bits: &[u8], code: u16) -> bool {
    let byte = usize::from(code / 8);
    let bit = u8::try_from(code % 8).expect("bit position is less than eight");
    bits.get(byte).is_some_and(|value| value & (1 << bit) != 0)
}

pub(crate) struct PageButtonInput {
    file_descriptor: libc::c_int,
}

impl PageButtonInput {
    /// Opens the exact reviewed Oasis GPIO-key device when present. Kindles
    /// without physical page buttons continue without this optional input.
    pub(crate) fn open() -> std::io::Result<Option<Self>> {
        for number in 0..32 {
            let path = format!("/dev/input/event{number}");
            let c_path = CString::new(path).expect("generated input path contains no NUL");
            // SAFETY: c_path is NUL-terminated and the returned descriptor is
            // owned here until it is closed or moved into PageButtonInput.
            let file_descriptor =
                unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
            if file_descriptor < 0 {
                continue;
            }
            if !Self::is_reviewed_input(file_descriptor) {
                // SAFETY: this candidate descriptor is still uniquely owned here.
                unsafe { libc::close(file_descriptor) };
                continue;
            }

            // The foreground framebuffer application owns the bezel keys just
            // as it owns the touchscreen. Release the grab in Drop.
            // SAFETY: file_descriptor refers to the reviewed evdev input.
            let grab_result = unsafe { libc::ioctl(file_descriptor, EVIOCGRAB as _, 1) };
            if grab_result < 0 {
                let error = std::io::Error::last_os_error();
                // SAFETY: PageButtonInput has not taken ownership after a failed grab.
                unsafe { libc::close(file_descriptor) };
                return Err(error);
            }
            return Ok(Some(Self { file_descriptor }));
        }
        Ok(None)
    }

    fn is_reviewed_input(file_descriptor: libc::c_int) -> bool {
        let mut name = [0_u8; 64];
        // EVIOCGNAME(64).
        // SAFETY: name is a writable fixed-size buffer for the duration of ioctl.
        let name_result = unsafe {
            input_ioctl(
                file_descriptor,
                ioctl_read_request(0x06, name.len()),
                name.as_mut_ptr(),
            )
        };
        if name_result < 0 {
            return false;
        }
        let name_length = name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(name.len());
        if &name[..name_length] != GPIO_KEYS_NAME {
            return false;
        }

        let mut input_id = InputId::default();
        // EVIOCGID.
        // SAFETY: input_id is writable storage of the size encoded in the request.
        let id_result = unsafe {
            input_ioctl(
                file_descriptor,
                ioctl_read_request(0x02, std::mem::size_of::<InputId>()),
                (&raw mut input_id).cast::<u8>(),
            )
        };
        if id_result < 0
            || input_id.bustype != BUS_HOST
            || input_id.vendor != REVIEWED_VENDOR
            || input_id.product != REVIEWED_PRODUCT
            || input_id.version != REVIEWED_VERSION
        {
            return false;
        }

        let mut key_bits = [0_u8; 16];
        // EVIOCGBIT(EV_KEY, 16).
        // SAFETY: key_bits is a writable fixed-size buffer for the ioctl.
        let bits_result = unsafe {
            input_ioctl(
                file_descriptor,
                ioctl_read_request(0x20 + libc::c_ulong::from(EVENT_KEY), key_bits.len()),
                key_bits.as_mut_ptr(),
            )
        };
        bits_result >= 0
            && bit_is_set(&key_bits, KEY_PAGE_UP)
            && bit_is_set(&key_bits, KEY_PAGE_DOWN)
    }

    pub(crate) const fn fd(&self) -> libc::c_int {
        self.file_descriptor
    }

    pub(crate) fn poll(&mut self, window: &MinimalSoftwareWindow) -> std::io::Result<()> {
        loop {
            let mut event = InputEvent {
                timestamp_seconds: 0,
                timestamp_microseconds: 0,
                kind: 0,
                code: 0,
                value: 0,
            };
            // SAFETY: event is initialized writable storage of exactly the size passed.
            let bytes_read = unsafe {
                libc::read(
                    self.file_descriptor,
                    (&raw mut event).cast::<libc::c_void>(),
                    std::mem::size_of::<InputEvent>(),
                )
            };
            if bytes_read < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(());
                }
                return Err(error);
            }
            if bytes_read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Kindle page-button input closed",
                ));
            }
            if usize::try_from(bytes_read).ok() != Some(std::mem::size_of::<InputEvent>()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Kindle page-button input returned a partial event",
                ));
            }

            if let Some((button, state)) = decoded_page_button(event.kind, event.code, event.value)
            {
                let text = match button {
                    PageButton::Previous => Key::PageUp.into(),
                    PageButton::Next => Key::PageDown.into(),
                };
                let window_event = match state {
                    ButtonState::Pressed => WindowEvent::KeyPressed { text },
                    ButtonState::Released => WindowEvent::KeyReleased { text },
                };
                let _ = window.try_dispatch_event(window_event);
            }
        }
    }
}

impl Drop for PageButtonInput {
    fn drop(&mut self) {
        // SAFETY: PageButtonInput uniquely owns this reviewed evdev descriptor.
        unsafe { libc::ioctl(self.file_descriptor, EVIOCGRAB as _, 0) };
        // SAFETY: the descriptor remains uniquely owned until this close.
        unsafe { libc::close(self.file_descriptor) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_page_keys_map_to_one_previous_or_next_action() {
        assert_eq!(
            decoded_page_button(EVENT_KEY, KEY_PAGE_UP, 1),
            Some((PageButton::Previous, ButtonState::Pressed))
        );
        assert_eq!(
            decoded_page_button(EVENT_KEY, KEY_PAGE_DOWN, 0),
            Some((PageButton::Next, ButtonState::Released))
        );
        assert_eq!(decoded_page_button(EVENT_KEY, KEY_PAGE_DOWN, 2), None);
        assert_eq!(decoded_page_button(0, KEY_PAGE_UP, 1), None);
        assert_eq!(decoded_page_button(EVENT_KEY, 30, 1), None);
    }

    #[test]
    fn capability_bitmap_uses_linux_key_code_positions() {
        let mut bits = [0_u8; 16];
        bits[usize::from(KEY_PAGE_UP / 8)] |= 1 << (KEY_PAGE_UP % 8);
        bits[usize::from(KEY_PAGE_DOWN / 8)] |= 1 << (KEY_PAGE_DOWN % 8);
        assert!(bit_is_set(&bits, KEY_PAGE_UP));
        assert!(bit_is_set(&bits, KEY_PAGE_DOWN));
        assert!(!bit_is_set(&bits, 30));
    }
}
