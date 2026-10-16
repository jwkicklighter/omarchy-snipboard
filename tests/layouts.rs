//! Real libxkbcommon translation; no compositor hooks or OS input injection.

use std::ffi::{CStr, c_char, c_int, c_void};

use snipboard::core::{Input, Matcher, synthetic_library};

#[repr(C)]
struct RuleNames {
    rules: *const c_char,
    model: *const c_char,
    layout: *const c_char,
    variant: *const c_char,
    options: *const c_char,
}

#[link(name = "xkbcommon")]
unsafe extern "C" {
    fn xkb_context_new(flags: c_int) -> *mut c_void;
    fn xkb_context_unref(context: *mut c_void);
    fn xkb_keymap_new_from_names(context: *mut c_void, names: *const RuleNames, flags: c_int) -> *mut c_void;
    fn xkb_keymap_unref(keymap: *mut c_void);
    fn xkb_keymap_key_by_name(keymap: *mut c_void, name: *const c_char) -> u32;
    fn xkb_state_new(keymap: *mut c_void) -> *mut c_void;
    fn xkb_state_unref(state: *mut c_void);
    fn xkb_state_update_key(state: *mut c_void, key: u32, direction: c_int) -> c_int;
    fn xkb_state_key_get_utf8(state: *mut c_void, key: u32, buffer: *mut c_char, size: usize) -> c_int;
}

const KEY_UP: c_int = 0;
const KEY_DOWN: c_int = 1;

#[test]
fn us_qwerty_and_dvorak_keymaps() {
    let snippets = synthetic_library(4).snippets;
    // SAFETY: plain libxkbcommon calls on objects created and released in this scope.
    unsafe {
        let context = xkb_context_new(0);
        assert!(!context.is_null());
        for (variant, keys) in [
            (c"", [c"AD03", c"AB07", c"AC01", c"AD08", c"AC09"]),
            (c"dvorak", [c"AC03", c"AB07", c"AC01", c"AC05", c"AD10"]),
        ] {
            let names = RuleNames {
                rules: c"evdev".as_ptr(),
                model: c"pc105".as_ptr(),
                layout: c"us".as_ptr(),
                variant: variant.as_ptr(),
                options: c"".as_ptr(),
            };
            let keymap = xkb_keymap_new_from_names(context, &names, 0);
            assert!(!keymap.is_null());
            let state = xkb_state_new(keymap);
            let translate = |key: &CStr| {
                let code = xkb_keymap_key_by_name(keymap, key.as_ptr());
                assert_ne!(code, 0, "{key:?}");
                let mut buffer = [0 as c_char; 32];
                xkb_state_key_get_utf8(state, code, buffer.as_mut_ptr(), buffer.len());
                CStr::from_ptr(buffer.as_ptr()).to_str().unwrap().to_owned()
            };
            let shift = xkb_keymap_key_by_name(keymap, c"LFSH".as_ptr());
            xkb_state_update_key(state, shift, KEY_DOWN);
            let mut text = translate(c"AE01");
            xkb_state_update_key(state, shift, KEY_UP);
            text.extend(keys.map(translate));
            assert_eq!(text, "!email");

            let mut matcher = Matcher::new(&snippets).unwrap();
            matcher.enabled = true;
            let layout = if variant.is_empty() { "qwerty" } else { variant.to_str().unwrap() };
            let input = Input { allowed: true, boundary: true, ..Input::default() };
            assert_eq!(matcher.feed(&text, Some(layout), input).as_deref(), Some("snippet-0000"));

            xkb_state_unref(state);
            xkb_keymap_unref(keymap);
        }
        xkb_context_unref(context);
    }
}
