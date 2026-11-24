"""Makes two fake account folders and a work folder under the folder given, for a test build."""
import json
import os
import shutil
import sys

ROOT = os.path.abspath(sys.argv[1])
WORK = os.path.join(ROOT, "work")
os.makedirs(ROOT, exist_ok=True)
shutil.copy(os.path.join(os.path.dirname(os.path.abspath(__file__)), "record.sh"), ROOT)
REC = os.path.join(ROOT, "record.sh").replace("\\", "/")

os.makedirs(WORK, exist_ok=True)

def account(name, keep, record):
    d = os.path.join(ROOT, name)
    os.makedirs(d, exist_ok=True)
    settings = {
        "apiKeyHelper": "echo sk-ant-fake-0000000000000000",
        "env": {
            "ANTHROPIC_BASE_URL": "http://127.0.0.1:8820",
            "DISABLE_TELEMETRY": "1",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
            "DISABLE_AUTOUPDATER": "1",
        },
        "model": "claude-opus-5-5",
    }
    if keep:
        settings["cleanupPeriodDays"] = 36500
    if record:
        settings["statusLine"] = {"type": "command", "command": f"bash '{REC}' status"}
        hook = lambda kind: [{"matcher": "*", "hooks": [{"type": "command", "command": f"bash '{REC}' {kind}"}]}]
        settings["hooks"] = {
            "PostToolUse": hook("post-tool-use"),
            "Stop": [{"hooks": [{"type": "command", "command": f"bash '{REC}' stop"}]}],
            "SubagentStart": hook("subagent-start"),
            "SubagentStop": hook("subagent-stop"),
            "PreCompact": hook("pre-compact"),
        }
    with open(os.path.join(d, "settings.json"), "w", newline="") as f:
        json.dump(settings, f, indent=2)
    projects = {}
    for key in (WORK, WORK.replace("\\", "/")):
        projects[key] = {"hasTrustDialogAccepted": True, "hasCompletedProjectOnboarding": True}
    state = {
        "hasCompletedOnboarding": True,
        "theme": "dark",
        "projects": projects,
        "bypassPermissionsModeAccepted": True,
    }
    with open(os.path.join(d, ".claude.json"), "w", newline="") as f:
        json.dump(state, f, indent=2)
    return d

print(account("acc-a", keep=False, record=True))
print(account("acc-b", keep=True, record=False))
print(WORK)
