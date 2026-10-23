#!/usr/bin/env python3
import os
import re
import subprocess
import sys

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    root_dir = os.path.abspath(os.path.join(script_dir, "../.."))
    index_html_path = os.path.join(root_dir, "index.html")
    mock_env_path = os.path.join(script_dir, "mock_environment.js")
    scenario_path = os.path.join(script_dir, "test_mesh_scenario.js")

    with open(mock_env_path, "r", encoding="utf-8") as f:
        mock_env = f.read()

    with open(index_html_path, "r", encoding="utf-8") as f:
        html = f.read()

    m = re.search(r'<script\b[^>]*>(.*?)</script>', html, re.DOTALL)
    if not m:
        print("ERROR: Could not find <script> in index.html", file=sys.stderr)
        sys.exit(1)
    index_script = m.group(1)

    with open(scenario_path, "r", encoding="utf-8") as f:
        scenario = f.read()

    runner_file = os.path.join(script_dir, ".generated_runner.js")
    with open(runner_file, "w", encoding="utf-8") as f:
        f.write(mock_env)
        f.write("\n\n// --- Extracted from index.html ---\n\n")
        f.write(index_script)
        f.write("\n\n// --- Test Scenario ---\n\n")
        f.write(scenario)

    # Choose JS engine: JSC (built-in on macOS) or Node
    jsc_bin = "/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc"
    if os.path.exists(jsc_bin):
        cmd = [jsc_bin, runner_file]
    else:
        cmd = ["node", runner_file]

    try:
        res = subprocess.run(cmd, check=True)
        sys.exit(res.returncode)
    finally:
        if os.path.exists(runner_file):
            os.remove(runner_file)

if __name__ == "__main__":
    main()
