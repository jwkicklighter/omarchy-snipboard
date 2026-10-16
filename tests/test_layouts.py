"""Real libxkbcommon translation; no compositor hooks or OS input injection."""
import ctypes as C
import unittest

from snipboard.core import Matcher, synthetic_library


class Names(C.Structure):
    _fields_ = [(name, C.c_char_p) for name in ("rules", "model", "layout", "variant", "options")]


class LayoutTests(unittest.TestCase):
    def test_us_qwerty_and_dvorak_keymaps(self):
        xkb = C.CDLL("libxkbcommon.so.0")
        signatures = {
            "xkb_context_new": ([C.c_int], C.c_void_p),
            "xkb_keymap_new_from_names": ([C.c_void_p, C.POINTER(Names), C.c_int], C.c_void_p),
            "xkb_state_new": ([C.c_void_p], C.c_void_p),
            "xkb_keymap_key_by_name": ([C.c_void_p, C.c_char_p], C.c_uint),
            "xkb_state_update_key": ([C.c_void_p, C.c_uint, C.c_int], C.c_int),
            "xkb_state_key_get_utf8": ([C.c_void_p, C.c_uint, C.c_char_p, C.c_size_t], C.c_int),
            "xkb_state_unref": ([C.c_void_p], None),
            "xkb_keymap_unref": ([C.c_void_p], None),
            "xkb_context_unref": ([C.c_void_p], None),
        }
        for name, (args, result) in signatures.items():
            function = getattr(xkb, name)
            function.argtypes, function.restype = args, result
        context = xkb.xkb_context_new(0)
        self.addCleanup(xkb.xkb_context_unref, context)
        for variant, keys in ((b"", [b"AD03", b"AB07", b"AC01", b"AD08", b"AC09"]),
                              (b"dvorak", [b"AC03", b"AB07", b"AC01", b"AC05", b"AD10"])):
            names = Names(b"evdev", b"pc105", b"us", variant, b"")
            keymap = xkb.xkb_keymap_new_from_names(context, C.byref(names), 0)
            self.assertTrue(keymap)
            self.addCleanup(xkb.xkb_keymap_unref, keymap)
            state = xkb.xkb_state_new(keymap)
            self.addCleanup(xkb.xkb_state_unref, state)
            buffer = C.create_string_buffer(32)
            def translate(key):
                code = xkb.xkb_keymap_key_by_name(keymap, key)
                self.assertTrue(code)
                xkb.xkb_state_key_get_utf8(state, code, buffer, 32)
                return buffer.value.decode()
            shift = xkb.xkb_keymap_key_by_name(keymap, b"LFSH")
            xkb.xkb_state_update_key(state, shift, 1)
            text = translate(b"AE01")
            xkb.xkb_state_update_key(state, shift, 0)
            text += "".join(translate(key) for key in keys)
            self.assertEqual(text, "!email")
            matcher = Matcher(synthetic_library(4)["snippets"])
            matcher.enabled = True
            self.assertEqual(matcher.feed(text, variant or b"qwerty", allowed=True, boundary=True), "snippet-0000")
