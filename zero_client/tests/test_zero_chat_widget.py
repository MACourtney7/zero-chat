#!/usr/bin/env python3
"""
Unit test runner for ZeroChatWidget in zero_chat.
Executes test_zero_chat_widget.js via JavaScriptCore (macOS) or Node.
"""

import os
import subprocess
import unittest

class TestZeroChatWidget(unittest.TestCase):
    def test_zero_chat_widget_headless_suite(self):
        tests_dir = os.path.dirname(os.path.abspath(__file__))
        js_test = os.path.join(tests_dir, "test_zero_chat_widget.js")
        repo_root = os.path.abspath(os.path.join(tests_dir, "../.."))

        jsc_bin = "/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc"
        if os.path.exists(jsc_bin):
            cmd = [jsc_bin, js_test]
        else:
            cmd = ["node", js_test]

        res = subprocess.run(cmd, cwd=repo_root, capture_output=True, text=True)
        self.assertEqual(
            res.returncode,
            0,
            f"ZeroChatWidget test suite failed:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}"
        )
        self.assertIn("ALL ZEROCATWIDGET UNIT TESTS PASSED SUCCESSFULLY!", res.stdout)

if __name__ == "__main__":
    unittest.main()
