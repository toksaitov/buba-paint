"""Unit tests for research maintenance scripts."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPTS_DIR = str(REPO_ROOT / "scripts")
if SCRIPTS_DIR not in sys.path:
    sys.path.insert(0, SCRIPTS_DIR)


def load_script(name: str, path: Path):
    """Load one script as a module."""
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MAINTENANCE = load_script("research_maintenance", REPO_ROOT / "scripts" / "research-maintenance.py")
DEPLOY = load_script("deploy_machine_for_tests", REPO_ROOT / "scripts" / "deploy-machine.py")
STOPPED_LIVE = load_script("deploy_stopped_live_for_tests", REPO_ROOT / "scripts" / "deploy-stopped-live.py")
PUBLISH_RESEARCH = load_script(
    "publish_research_images_for_tests",
    REPO_ROOT / "scripts" / "publish-research-images.py",
)
PUBLISH_LIVE = load_script(
    "publish_live_images_for_tests",
    REPO_ROOT / "scripts" / "publish-live-images.py",
)


def sample_lock(input_hashes: dict[str, str] | None = None) -> dict:
    """Return a valid digest-pinned lock fixture."""
    return {
        "schema": 1,
        "input_hashes": input_hashes or {"dashboard": "stale", "research_worker": "stale"},
        "images": {
            "dashboard": {
                "ref": "ghcr.io/toksaitov/buba-paint-dashboard@sha256:" + "a" * 64,
                "digest": "sha256:" + "a" * 64,
            },
            "research_worker": {
                "ref": "ghcr.io/toksaitov/buba-paint-research-worker@sha256:" + "b" * 64,
                "digest": "sha256:" + "b" * 64,
            },
        },
    }


def sample_live_lock(input_hashes: dict[str, str] | None = None) -> dict:
    """Return a valid stopped-live image lock fixture."""
    return {
        "schema": 1,
        "input_hashes": input_hashes
        or {
            "agent": "stale",
            "dashboard": "stale",
            "paint": "stale",
            "sidecar": "stale",
        },
        "images": {
            "agent": {
                "ref": "ghcr.io/toksaitov/buba-paint-agent@sha256:" + "a" * 64,
                "digest": "sha256:" + "a" * 64,
            },
            "dashboard": {
                "ref": "ghcr.io/toksaitov/buba-paint-dashboard@sha256:" + "b" * 64,
                "digest": "sha256:" + "b" * 64,
            },
            "paint": {
                "ref": "ghcr.io/toksaitov/buba-paint-bot@sha256:" + "c" * 64,
                "digest": "sha256:" + "c" * 64,
            },
            "sidecar": {
                "ref": "ghcr.io/toksaitov/buba-paint-sidecar@sha256:" + "d" * 64,
                "digest": "sha256:" + "d" * 64,
            },
        },
    }


class ResearchMaintenanceTests(unittest.TestCase):
    """Covers local-only maintenance helpers."""

    def test_redact_secret_text_redacts_env_and_json(self) -> None:
        """Redacts secret-looking keys in env and JSON text."""
        text = 'ADMIN_PASSWORD=abc\nBUBA_RESEARCH_WORKER_TOKEN=def\nSAFE=value\n{"jwt_secret":"ghi"}'
        redacted = MAINTENANCE.redact_secret_text(text)
        self.assertNotIn("abc", redacted)
        self.assertNotIn("def", redacted)
        self.assertNotIn("ghi", redacted)
        self.assertIn("SAFE=value", redacted)

    def test_validate_backup_id_rejects_path_traversal(self) -> None:
        """Rejects backup ids that could escape the backup directory."""
        MAINTENANCE.validate_backup_id("dashboard-db-20260531-120000")
        with self.assertRaises(RuntimeError):
            MAINTENANCE.validate_backup_id("../dashboard.db")

    def test_parse_backup_manifest_text_validates_required_fields(self) -> None:
        """Parses only complete successful backup manifests."""
        manifest = {
            "backup_id": "dashboard-db-20260531-120000",
            "sha256": "1" * 64,
            "quick_check": "ok",
            "db_bytes": 123,
        }
        self.assertEqual(
            MAINTENANCE.parse_backup_manifest_text(json.dumps(manifest))["backup_id"],
            "dashboard-db-20260531-120000",
        )
        manifest["quick_check"] = "malformed"
        with self.assertRaises(RuntimeError):
            MAINTENANCE.parse_backup_manifest_text(json.dumps(manifest))

    def test_validate_lock_payload_requires_digest_refs(self) -> None:
        """Rejects mutable rollback image refs."""
        lock = sample_lock()
        MAINTENANCE.validate_lock_payload(lock)
        lock["images"]["dashboard"]["ref"] = "ghcr.io/toksaitov/buba-paint-dashboard:latest"
        with self.assertRaises(RuntimeError):
            MAINTENANCE.validate_lock_payload(lock)

    def test_lock_from_ref_loads_explicit_lock_file(self) -> None:
        """Loads rollback locks from explicit file paths."""
        lock = sample_lock()
        with tempfile.TemporaryDirectory() as tmp_dir:
            path = Path(tmp_dir) / "lock.json"
            path.write_text(json.dumps(lock), encoding="utf-8")
            loaded, source = MAINTENANCE.lock_from_ref(str(path))
        self.assertEqual(source, str(path))
        self.assertEqual(loaded["images"]["dashboard"]["digest"], "sha256:" + "a" * 64)

    def test_deploy_image_lock_allows_stale_only_when_explicit(self) -> None:
        """Allows stale rollback locks only under the explicit override path."""
        lock = sample_lock()
        with tempfile.TemporaryDirectory() as tmp_dir:
            path = Path(tmp_dir) / "lock.json"
            path.write_text(json.dumps(lock), encoding="utf-8")
            DEPLOY.validate_image_lock(lock, path, allow_stale=True)
            with self.assertRaises(ValueError):
                DEPLOY.validate_image_lock(lock, path, allow_stale=False)

    def test_stopped_live_lock_requires_all_digest_refs(self) -> None:
        """Rejects stopped-live locks with missing or mutable images."""
        lock = sample_live_lock(STOPPED_LIVE.all_image_input_hashes(REPO_ROOT, tuple(STOPPED_LIVE.LIVE_IMAGE_ENVS)))
        with tempfile.TemporaryDirectory() as tmp_dir:
            path = Path(tmp_dir) / "lock.json"
            STOPPED_LIVE.validate_image_lock(lock, path)
            del lock["images"]["sidecar"]
            with self.assertRaises(ValueError):
                STOPPED_LIVE.validate_image_lock(lock, path)

    def test_stopped_live_deploy_exports_only_digest_pinned_images(self) -> None:
        """Builds live image exports from the lock without mutable tags."""
        lock = sample_live_lock(STOPPED_LIVE.all_image_input_hashes(REPO_ROOT, tuple(STOPPED_LIVE.LIVE_IMAGE_ENVS)))
        exports = STOPPED_LIVE.locked_image_exports(lock)
        self.assertIn("export BUBA_AGENT_IMAGE=", exports)
        self.assertIn("@sha256:" + "a" * 64, exports)
        self.assertNotIn(":latest", exports)

    def test_stopped_live_preflight_requires_checksum_and_stopped_bot(self) -> None:
        """Preflight script validates the runtime, checksum, and stopped bot services."""
        script = STOPPED_LIVE.preflight_remote_script(
            "/home/ubuntu/buba-paint-live",
            "live-readonly-20260514-184119",
            "2f" * 32,
        )
        self.assertIn("sha256sum \"$BUBA_RUNTIME_DIR/paint.db\"", script)
        self.assertIn("buba-paint-(paint|sidecar)-1", script)
        self.assertIn("test -z \"$running_bot\"", script)

    def test_publishers_reject_all_dirty_paths_by_default(self) -> None:
        """Publishers treat every visible dirty path as provenance-affecting."""
        status = "\n".join(
            [
                " M data/experiments/manual-note.txt",
                " M dashboard/client/src/App.tsx",
            ]
        )
        for publisher in (PUBLISH_RESEARCH, PUBLISH_LIVE):
            with mock.patch.object(publisher, "output", return_value=status):
                self.assertEqual(
                    publisher.dirty_source_files(),
                    [
                        " M data/experiments/manual-note.txt",
                        " M dashboard/client/src/App.tsx",
                    ],
                )
                with self.assertRaises(RuntimeError):
                    publisher.ensure_clean_source(allow_dirty=False)
                self.assertEqual(
                    publisher.ensure_clean_source(allow_dirty=True),
                    [
                        " M data/experiments/manual-note.txt",
                        " M dashboard/client/src/App.tsx",
                    ],
                )

    def test_remote_python_script_quotes_untrusted_values(self) -> None:
        """Quotes root and extra values before embedding remote shell commands."""
        script = MAINTENANCE.remote_python_script(
            "/tmp/research root; echo unsafe",
            "print('ok')",
            {"BACKUP_ID": "dashboard-db-20260531-120000"},
        )
        self.assertIn("ROOT='/tmp/research root; echo unsafe'", script)
        self.assertIn("BACKUP_ID=dashboard-db-20260531-120000", script)

    def test_command_failure_shapes_diagnostics_payload(self) -> None:
        """Converts deploy failures into bounded JSON-friendly diagnostics."""
        error = subprocess.CalledProcessError(
            2,
            ["deploy"],
            output="x" * 25000,
            stderr="y" * 5000,
        )
        payload = MAINTENANCE.command_failure(error)
        self.assertFalse(payload["ok"])
        self.assertEqual(payload["returncode"], 2)
        self.assertEqual(len(payload["stdout"]), 20000)
        self.assertEqual(len(payload["stderr"]), 5000)


if __name__ == "__main__":
    unittest.main()
