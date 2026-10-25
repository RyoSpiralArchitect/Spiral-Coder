"""Offline contracts for the real-repository lane; no Rust builds or provider calls."""
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from coupled_bench.common import inventory, read_json, sha, write_json
from self_dogfood import assessment, contracts, evidence, process, runner


def spec():
    return {"version": 1, "id": "restore-handoff", "prompt": "prompt.txt",
        "editable": ["src/recovery.rs"], "protected": ["Cargo.toml"],
        "native_checks": ["cargo test --locked recovery"],
        "roles": {"coder": {"provider": "openai", "model": "fixture-coder"},
                  "observer": {"provider": "gemini", "model": "fixture-observer"}},
        "max_requests": 12, "coder_chunk_requests": 4, "wall_seconds": 30,
        "evaluation_wall_seconds": 30, "timeout_seconds": 10, "max_tokens": 4096,
        "temperature": 0.1,
        "probes": [{"source": "probe.rs", "target": "src/recovery.rs", "mode": "append"}],
        "reference": [{"source": "reference.rs", "target": "src/recovery.rs", "mode": "replace"}],
        "checks": [
            {"id": "build", "kind": "build", "baseline": "pass", "argv": ["cargo", "build", "--locked"], "timeout_seconds": 10},
            {"id": "regression", "kind": "regression", "baseline": "pass", "argv": ["cargo", "test", "existing"], "timeout_seconds": 10, "minimum_passed_tests": 8},
            {"id": "improvement", "kind": "improvement", "baseline": "fail", "argv": ["cargo", "test", "new-probe"], "timeout_seconds": 10,
             "expected_test_count": 6, "baseline_failure_contains": ["restore_after_read", "5 passed; 1 failed"]}]}


class LaneTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.repo = self.root / "repo"
        (self.repo / "src").mkdir(parents=True)
        (self.repo / "tests").mkdir()
        (self.repo / "src/recovery.rs").write_text("broken baseline\n")
        (self.repo / "tests/existing.rs").write_text("original regression\n")
        (self.repo / "Cargo.toml").write_text("original manifest\n")
        (self.repo / ".gitattributes").write_text("tests export-ignore\nsrc/recovery.rs export-subst\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("-c", "user.name=Fixture", "-c", "user.email=fixture@localhost", "commit", "-qm", "baseline")
        self.commit = self.git("rev-parse", "HEAD").strip()
        self.inputs = self.root / "inputs"
        self.inputs.mkdir()
        (self.inputs / "prompt.txt").write_text("Fix recovery without touching tests.\n")
        (self.inputs / "probe.rs").write_text("\nheld-out probe\n")
        (self.inputs / "reference.rs").write_text("fixed reference\n")
        self.spec = spec()
        write_json(self.inputs / "task.json", self.spec)
        self.binary = self.root / "executor"
        self.binary.write_text("fake frozen executor\n")
        self.out = self.root / "episode"
        self.observed_checks = []

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, text=True, stderr=subprocess.PIPE)

    def fake_check(self, argv, workspace, env, timeout, log, secrets=()):
        self.assertNotIn("OPENAI_API_KEY", env)
        self.assertNotIn("GEMINI_API_KEY", env)
        self.assertEqual(env["CARGO_INCREMENTAL"], "0")
        self.assertFalse(Path(env["CARGO_TARGET_DIR"]).is_relative_to(workspace))
        self.observed_checks.append((argv, (workspace / "tests/existing.rs").read_text(), (workspace / "src/recovery.rs").read_text()))
        code, output = 0, ""
        if argv[:2] == ["cargo", "build"]:
            binary = Path(env["CARGO_TARGET_DIR"]) / "debug/spiral-coder"
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_text((workspace / "src/recovery.rs").read_text())
        elif "new-probe" in argv:
            if "fixed" in (workspace / "src/recovery.rs").read_text():
                output = "running 6 tests\ntest result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out;\n"
            else:
                code = 101
                output = "restore_after_read FAILED\nrunning 6 tests\ntest result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out;\n"
        else:
            output = "test result: ok. 8 passed; 0 failed; 1 ignored; 0 measured; 6 filtered out;\n"
        log.parent.mkdir(parents=True, exist_ok=True)
        log.write_text(output)
        return {"exit_code": code, "timed_out": False, "output_limit": False, "wall_seconds": 0, "log_redactions": 0}

    def prepare(self):
        args = SimpleNamespace(repo=self.repo, baseline_ref=self.commit, runtime_commit=self.commit,
                               binary=self.binary, spec=self.inputs / "task.json", out=self.out, target_dir=None)
        with patch.object(assessment, "run_process", self.fake_check):
            result = runner.prepare(args)
        self.pin = result["declaration_sha256"]
        return result

    def args(self, run_pin=None):
        return SimpleNamespace(out=self.out, declaration_sha256=self.pin, run_sha256=run_pin)

    def fake_runtime(self, argv, workspace, env, timeout, log, secrets=(), observer=True, commit_candidate=False):
        self.assertEqual(set(env) & {"OPENAI_API_KEY", "GEMINI_API_KEY", "MISTRAL_API_KEY"}, {"OPENAI_API_KEY", "GEMINI_API_KEY"})
        self.assertEqual(secrets, ["fixture-openai-key", "fixture-gemini-key"])
        self.assertEqual([env["OPENAI_API_KEY"], env["GEMINI_API_KEY"]], secrets)
        self.assertNotIn("fixture-openai-key", argv)
        self.assertIn("--observer-provider", argv)
        if "observer_max_tokens" in self.spec:
            self.assertEqual(argv[argv.index("--observer-max-tokens") + 1], str(self.spec["observer_max_tokens"]))
        else:
            self.assertNotIn("--observer-max-tokens", argv)
        (workspace / "src/recovery.rs").write_text("fixed by candidate\n")
        if commit_candidate:
            subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@localhost", "commit", "-qam", "candidate commit"], cwd=workspace, check=True, stdout=subprocess.PIPE)
        run = self.out / "episode/run"
        (run / "state").mkdir(parents=True)
        work = {"task_id": "fixture-task", "root": (self.out / "episode/task.txt").read_text().strip(),
                "tool_root": str(workspace), "request_limit": 12,
                "requests": [{"id": 1, "role": "coder", "status": "ok"}] + ([{"id": 2, "role": "observer", "status": "ok"}] if observer else [])}
        roles = {}
        for name, provider, model, origin in [("coder", "openai", "fixture-coder", "https://api.openai.com"), ("observer", "gemini", "fixture-observer", "https://generativelanguage.googleapis.com")]:
            roles[name] = {"provider": provider, "transport": "openai-compatible", "model": model, "endpoint_origin": origin,
                           "temperature": 0.1 if name == "coder" else 0.2, "max_tokens": self.spec["max_tokens"] if name == "coder" else self.spec.get("observer_max_tokens", min(self.spec["max_tokens"], 1200)), "timeout_seconds": 10}
        actions = [{"tool": "patch_file", "executed": True, "succeeded": True, "result_step": 1},
                   {"tool": "exec", "executed": True, "succeeded": True, "arguments": {"command": self.spec["native_checks"][0]}, "result_step": 2},
                   {"tool": "done", "executed": True, "result": "OK: done", "arguments": {"remaining_acceptance": []}, "result_step": 3}]
        report = {"completed": True, "roles": roles, "same_model_both_roles": False, "coder_chunk_requests": 4,
                  "role_requests": {"coder": 1, "observer": int(observer)}, "work": work,
                  "execution": {"actions": actions, "mutation_step": 1, "pending_exact_checks": []}}
        write_json(run / "report.json", report)
        write_json(run / "state/tui-work.json", work)
        log.write_text("fake native execution\n")
        return {"exit_code": 0, "timed_out": False, "output_limit": False, "log_redactions": 0, "wall_seconds": 0}

    def execute(self, observer=True, commit_candidate=False):
        def fake(*args):
            return self.fake_runtime(*args, observer=observer, commit_candidate=commit_candidate)
        with patch.dict(os.environ, {"OPENAI_API_KEY": "fixture-openai-key", "GEMINI_API_KEY": "fixture-gemini-key", "MISTRAL_API_KEY": "unused-secret"}), patch.object(runner, "run_process", fake):
            return runner.run(self.args())

    def assess(self, run_pin):
        with patch.object(assessment, "run_process", self.fake_check):
            return runner.evaluate(self.args(run_pin))

    def test_freezes_full_git_tree_without_dirty_or_export_changes(self):
        (self.repo / "src/recovery.rs").write_text("dirty source must not enter\n")
        (self.repo / "untracked.txt").write_text("not baseline\n")
        result = self.prepare()
        self.assertEqual(result["baseline_commit"], self.commit)
        self.assertEqual((self.out / "baseline/src/recovery.rs").read_text(), "broken baseline\n")
        self.assertTrue((self.out / "baseline/tests/existing.rs").exists())
        self.assertFalse((self.out / "baseline/untracked.txt").exists())
        self.assertNotIn("held-out probe", (self.out / "episode/workspace/src/recovery.rs").read_text())
        self.assertTrue(all("held-out probe" in item[2] for item in self.observed_checks))
        declaration, _ = contracts.load_frozen(self.out, self.pin)
        self.assertIn("tests/existing.rs", declaration["protected_paths"])

    def test_complete_episode_qualifies_and_candidate_commit_does_not_hide_patch(self):
        self.prepare()
        ran = self.execute(commit_candidate=True)
        patch_text = (self.out / "episode/candidate.patch").read_text()
        self.assertIn("+fixed by candidate", patch_text)
        result = self.assess(ran["run_receipt_sha256"])
        self.assertTrue(result["qualified_candidate"], result["gates"])
        self.assertEqual(result["request_roles"], {"coder": 1, "observer": 1})
        with self.assertRaisesRegex(ValueError, "Assessment already"):
            self.assess(ran["run_receipt_sha256"])

    def test_explicit_observer_token_budget_is_independent_of_coder_budget(self):
        self.spec.update(observer_max_tokens=8192, max_tokens=1024)
        write_json(self.inputs / "task.json", self.spec)
        self.prepare()
        ran = self.execute()
        result = self.assess(ran["run_receipt_sha256"])
        self.assertTrue(result["qualified_candidate"], result["gates"])
        receipt = read_json(self.out / "assessment-001.json")
        self.assertEqual(receipt["native_roles"]["observer"]["max_tokens"], 8192)
        self.assertEqual(receipt["native_roles"]["coder"]["max_tokens"], 1024)

    def test_explicit_observer_token_budget_rejects_blank_noninteger_and_out_of_range(self):
        for value in ("", " ", "8192", None, True, 0, -1, 65537):
            with self.subTest(value=value):
                task = copy.deepcopy(self.spec)
                task["observer_max_tokens"] = value
                with self.assertRaisesRegex(ValueError, "observer_max_tokens must be 1..65536"):
                    contracts.validate(task)

    def test_coder_only_completion_is_not_heterogeneous_qualification(self):
        self.prepare()
        ran = self.execute(observer=False)
        result = self.assess(ran["run_receipt_sha256"])
        self.assertFalse(result["qualified_candidate"])
        self.assertFalse(result["gates"]["both_roles_called"])
        self.assertTrue(result["gates"]["independent_checks_passed"])

    def test_preserves_interrupted_wire_accounting_without_qualifying(self):
        self.prepare()
        self.execute()
        (self.out / "episode/run/report.json").unlink()
        (self.out / "episode/process.json").unlink()
        result = self.assess(None)
        self.assertEqual(result["requests"], 2)
        self.assertFalse(result["gates"]["pinned_run_receipt"])
        self.assertFalse(result["qualified_candidate"])

    def test_source_only_repair_cannot_omit_required_documentation_change(self):
        self.spec["editable"].append("docs/behavior.md")
        self.spec["required_changed_paths"] = ["docs/behavior.md"]
        write_json(self.inputs / "task.json", self.spec)
        self.prepare()
        ran = self.execute()
        result = self.assess(ran["run_receipt_sha256"])
        self.assertFalse(result["qualified_candidate"])
        self.assertFalse(result["gates"]["required_paths_changed"])
        self.assertTrue(result["gates"]["independent_checks_passed"])

    def test_required_documentation_change_can_qualify(self):
        self.spec["editable"].append("docs/behavior.md")
        self.spec["required_changed_paths"] = ["docs/behavior.md"]
        write_json(self.inputs / "task.json", self.spec)
        self.prepare()
        self.execute()
        doc = self.out / "episode/workspace/docs/behavior.md"
        doc.parent.mkdir()
        doc.write_text("Document the restored recovery behavior.\n")
        run_pin = evidence.seal_run(self.out, self.pin)
        result = self.assess(run_pin)
        self.assertTrue(result["gates"]["required_paths_changed"])
        self.assertTrue(result["qualified_candidate"], result["gates"])

    def test_truncated_report_retains_durable_request_count_and_error_receipt(self):
        self.prepare()
        self.execute()
        (self.out / "episode/run/report.json").write_text('{"completed": true, "work":')
        run_pin = evidence.seal_run(self.out, self.pin)
        result = self.assess(run_pin)
        self.assertEqual(result["requests"], 2)
        self.assertEqual(result["request_roles"], {"coder": 1, "observer": 1})
        self.assertFalse(result["qualified_candidate"])
        self.assertFalse(result["gates"]["native_receipts_valid"])
        receipt = read_json(self.out / "assessment-001.json")
        self.assertEqual(receipt["native_evidence_source"], "durable_ledger")
        self.assertEqual(receipt["receipt_errors"][0]["path"], "episode/run/report.json")
        self.assertIn("JSONDecodeError", receipt["receipt_errors"][0]["reason"])
        self.assertTrue((self.out / "raw-manifest-001.json").exists())

    def test_malformed_report_ledger_and_process_leave_request_count_unknown(self):
        self.prepare()
        self.execute()
        for name, body in [("run/report.json", "[]"), ("run/state/tui-work.json", '{"requests":'), ("process.json", "null")]:
            (self.out / "episode" / name).write_text(body)
        result = self.assess(None)
        self.assertIsNone(result["requests"])
        self.assertFalse(result["qualified_candidate"])
        receipt = read_json(self.out / "assessment-001.json")
        self.assertEqual(receipt["native_evidence_source"], "unavailable")
        self.assertEqual(len(receipt["receipt_errors"]), 3)

    def test_padded_keys_share_normalized_environment_and_redaction_bytes(self):
        self.prepare()
        def padded(*args):
            result = self.fake_runtime(*args)
            (self.out / "episode/key-echo.txt").write_text("fixture-openai-key")
            return result
        with patch.dict(os.environ, {"OPENAI_API_KEY": " \tfixture-openai-key\n", "GEMINI_API_KEY": " fixture-gemini-key "}), patch.object(runner, "run_process", padded):
            result = runner.run(self.args())
        self.assertIn("key-echo.txt", result["redacted_files"])
        self.assertEqual((self.out / "episode/key-echo.txt").read_text(), "[REDACTED]")

    def test_whitespace_only_provider_key_is_rejected_before_run_starts(self):
        self.prepare()
        with patch.dict(os.environ, {"OPENAI_API_KEY": " \n\t", "GEMINI_API_KEY": "fixture-gemini-key"}):
            with self.assertRaisesRegex(ValueError, "blank OPENAI_API_KEY"):
                runner.run(self.args())
        self.assertFalse((self.out / "episode/started.json").exists())

    def test_scope_violation_never_enters_verifier_copy(self):
        self.prepare()
        ran = self.execute()
        (self.out / "episode/workspace/tests/existing.rs").write_text("waived test\n")
        # A changed candidate cannot reuse an earlier pin. Simulate a run whose
        # original post-run receipt already contained the scope violation.
        run_pin = evidence.seal_run(self.out, self.pin)
        self.observed_checks.clear()
        result = self.assess(run_pin)
        self.assertFalse(result["gates"]["strict_scope"])
        self.assertFalse(result["gates"]["original_protected_files_preserved"])
        self.assertTrue(all(row[1] == "original regression\n" for row in self.observed_checks))

    def test_coordinated_declaration_and_adjacent_hash_edit_cannot_replace_pin(self):
        self.prepare()
        declaration = read_json(self.out / "predeclared.json")
        declaration["max_requests"] = 999
        write_json(self.out / "predeclared.json", declaration)
        (self.out / "predeclared.sha256").write_text(sha(self.out / "predeclared.json"))
        with self.assertRaisesRegex(ValueError, "operator-pinned"):
            contracts.load_frozen(self.out, self.pin)

    def test_swapped_native_receipts_are_rejected_before_assessment(self):
        self.prepare()
        ran = self.execute()
        report = self.out / "episode/run/report.json"
        report.write_text(report.read_text().replace("fixture-observer", "unrelated-model"))
        with self.assertRaisesRegex(ValueError, "artifacts changed"):
            self.assess(ran["run_receipt_sha256"])

    def test_frozen_prompt_checked_after_run(self):
        self.prepare()
        (self.out / "episode/task.txt").write_text("a different task")
        with self.assertRaisesRegex(ValueError, "task receipt"):
            contracts.load_frozen(self.out, self.pin)

    def test_no_in_place_run_retry(self):
        self.prepare()
        self.execute()
        with self.assertRaisesRegex(ValueError, "already started"):
            self.execute()

    def test_startup_banner_and_early_exit_do_not_pass_probe(self):
        self.prepare()
        def early(argv, workspace, env, timeout, log):
            log.parent.mkdir(parents=True, exist_ok=True)
            log.write_text("running 6 tests\n")
            return {"exit_code": 0, "timed_out": False, "output_limit": False, "wall_seconds": 0}
        with patch.object(assessment, "run_process", early):
            rows = assessment.checks(self.out, self.spec, "early-exit")
        self.assertFalse(rows[2]["passed"])
        self.assertFalse(rows[1]["passed"])

    def test_wrong_baseline_test_failure_count_rejects_preflight(self):
        original = self.fake_check
        def wrong(*args):
            result = original(*args)
            if result["exit_code"] == 101:
                log = args[4]
                log.write_text(log.read_text().replace("5 passed; 1 failed", "4 passed; 2 failed"))
            return result
        self.fake_check = wrong
        with self.assertRaisesRegex(ValueError, "Baseline/reference qualification failed"):
            self.prepare()
        self.assertFalse(read_json(self.out / "preflight.json")["baseline_expected"])
        self.assertFalse((self.out / "episode/started.json").exists())

    def test_failure_identity_requires_a_nonempty_list_of_nonblank_strings(self):
        for check_index in (0, 2):
            for value in ("restores_fix", "", None, True, 1, {}, [], [""], [" "], [1], ["restore_after_read", None]):
                with self.subTest(check_index=check_index, value=value):
                    task = copy.deepcopy(self.spec)
                    task["checks"][check_index]["baseline_failure_contains"] = value
                    with self.assertRaisesRegex(ValueError, "baseline_failure_contains"):
                        contracts.validate(task)

    def test_failing_baseline_requires_failure_identity(self):
        task = copy.deepcopy(self.spec)
        del task["checks"][2]["baseline_failure_contains"]
        with self.assertRaisesRegex(ValueError, "baseline_failure_contains"):
            contracts.validate(task)

    def test_unrelated_failure_with_expected_count_rejects_preflight(self):
        self.spec["checks"][2]["baseline_failure_contains"] = ["restores_fix"]
        write_json(self.inputs / "task.json", self.spec)
        original = self.fake_check

        def wrong(*args):
            result = original(*args)
            if result["exit_code"] == 101:
                log = args[4]
                log.write_text(log.read_text().replace("restore_after_read", "existing_proof_rejects_stale_read"))
            return result

        self.fake_check = wrong
        with self.assertRaisesRegex(ValueError, "Baseline/reference qualification failed"):
            self.prepare()
        self.assertFalse(read_json(self.out / "preflight.json")["baseline_expected"])
        self.assertFalse((self.out / "episode/started.json").exists())

    def test_paths_same_family_and_unknown_contracts_rejected(self):
        for mutate in [lambda s: s["editable"].append("../outside"),
                       lambda s: s["roles"]["observer"].update(provider="openai"),
                       lambda s: s["checks"][0].update(waive=True),
                       lambda s: s.update(max_requests=True),
                       lambda s: s.update(required_changed_paths=["outside.rs"]),
                       lambda s: s.update(required_changed_paths=["src/recovery.rs", "src/recovery.rs"]),
                       lambda s: s.update(required_changed_paths="src/recovery.rs")]:
            task = copy.deepcopy(self.spec)
            mutate(task)
            with self.assertRaises(ValueError):
                contracts.validate(task)


class ProcessTests(unittest.TestCase):
    def test_timeout_cancels_descendant_after_parent_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "escaped"
            child = f"import time,pathlib;time.sleep(0.7);pathlib.Path({str(marker)!r}).write_text('bad')"
            parent = "import subprocess,sys;subprocess.Popen([sys.executable,'-S','-c'," + repr(child) + "])"
            row = process.run_process([sys.executable, "-S", "-c", parent], root, os.environ.copy(), 1, root / "log")
            self.assertEqual(row["exit_code"], 0)
            time.sleep(0.8)
            self.assertFalse(marker.exists())

    def test_wall_cap_and_log_cap_and_secret_redaction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "log"
            row = process.run_process([sys.executable, "-S", "-c", "import time;time.sleep(5)"], root, os.environ.copy(), 0.05, log)
            self.assertTrue(row["timed_out"])
            secret = "fixture-private-value"
            program = "import sys;sys.stdout.write('x'*65530+" + repr(secret) + "+'tail')"
            row = process.run_process([sys.executable, "-S", "-c", program], root, os.environ.copy(), 2, log, [secret])
            self.assertNotIn(secret, log.read_text())
            self.assertEqual(row["log_redactions"], 1)
            with patch.object(process, "MAX_LOG_BYTES", 1024):
                row = process.run_process([sys.executable, "-S", "-c", "print('x'*100000)"], root, os.environ.copy(), 2, log)
            self.assertTrue(row["output_limit"])
            self.assertLessEqual(log.stat().st_size, 1024)


if __name__ == "__main__":
    unittest.main()
