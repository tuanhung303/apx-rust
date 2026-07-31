#!/usr/bin/env python3
"""Parser-fixture parity generator/verifier against the frozen Go oracle.

The Go repository at tuanhung303/apx is the behavioral oracle. Its parser
surface is frozen at REQUIRED_SHA (recorded in fixtures/corpus/parser.json and
pinned by the CI parity job, which checks out that exact revision). Every
command below regenerates the corpus from a detached worktree at REQUIRED_SHA,
so the local Go checkout may keep moving without silently changing the oracle.

Commands:
  generate      write fixtures/corpus/parser.json from the frozen oracle
  verify        regenerate to a temp file and byte-compare the committed corpus
  verify-rust   run the Rust differential test against the committed corpus
  test-safety   exercise the generator's refuse-to-overwrite guard
"""

import argparse
import difflib
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REQUIRED_SHA = "bcd85fc2f817e7c405f8b92953cd3ad4db165759"
TEST_FILENAME = "fixtures_generation_test.go"

# Each case is a (name, script) pair. Scripts become Go string literals in the
# generated test; go_quote() preserves exact bytes (raw UTF-8 for non-ASCII).
# DYNAMIC_SCRIPTS replaces the script with a Go expression (large payloads).
CASES = [
    # --- path commands ------------------------------------------------------
    ("in_simple", "in file.txt\n"),
    ("new_simple", "new file.txt\n"),
    ("mv_simple", "mv file.txt\n"),
    ("rm_simple", "rm file.txt\n"),
    ("rm_no_path", "rm\n"),
    ("in_no_space", "in\n"),
    ("new_no_space", "new\n"),
    ("mv_no_space", "mv\n"),
    ("in_empty_path", "in \n"),
    ("new_empty_path", "new \n"),
    ("mv_empty_path", "mv \n"),
    ("rm_empty_path", "rm \n"),
    ("path_cleaning", "in a/b/../c/./d\n"),
    ("path_spaces", "in a b.txt\n"),
    ("path_trailing_space", "in file.txt \n"),
    ("path_dot", "in .\n"),
    ("path_dotdot", "in ..\n"),
    ("path_double_slash", "in a//b/\n"),
    ("path_absolute", "in /tmp/x\n"),
    ("path_unicode", "in 日本語/файл.txt\n"),
    ("path_tab_lead", "\tin file.txt\n"),
    ("path_control", "in\x01file\n"),
    ("multiple_commands", "in file.txt\nrm\nnew other.txt\n"),
    ("empty_and_whitespace_lines", "\n  \nin file.txt\n\n\t\nrm\n"),
    ("nbsp_line", "\u00a0\nin file.txt\n"),
    ("crlf_line_endings", "in file.txt\r\nrm\r\n"),
    ("cr_only_terminator", "in file.txt\r"),
    # --- bare commands ------------------------------------------------------
    ("bare_all", "del\ncopy\ncut\npaste\ncommit\n"),
    ("unknown_command", "unknown_command\n"),
    ("case_sensitive", "IN file.txt\n"),
    # --- sel ----------------------------------------------------------------
    ("sel_simple", "sel 1 1:2\n"),
    ("sel_multi", "sel 12 3:5\n"),
    ("sel_bad_line", "sel x 1:2\n"),
    ("sel_zero_line", "sel 0 1:2\n"),
    ("sel_leading_zero_line", "sel 01 1:2\n"),
    ("sel_zero_start", "sel 1 0:2\n"),
    ("sel_leading_zero_start", "sel 1 01:2\n"),
    ("sel_zero_end", "sel 1 2:0\n"),
    ("sel_reversed", "sel 1 4:2\n"),
    ("sel_bad_column", "sel 1 2:x\n"),
    ("sel_trailing", "sel 1 2:3 4\n"),
    ("sel_no_space", "sel1 2:3\n"),
    ("sel_line_huge", "sel 99999999999999999999 1:2\n"),
    ("sel_number_huge", "sel 1 99999999999999999999:2\n"),
    ("sel_control_line", "sel \x01 1:2\n"),
    # --- tsel ---------------------------------------------------------------
    ("tsel_simple", "tsel 2 \"old\"\n"),
    ("tsel_count", "tsel 2 \"old\" 3\n"),
    ("tsel_empty", "tsel 1 \"\" 1\n"),
    ("tsel_count_zero", "tsel 1 \"x\" 0\n"),
    ("tsel_count_leading_zero", "tsel 1 \"x\" 02\n"),
    ("tsel_count_huge", "tsel 1 \"x\" 99999999999999999999\n"),
    ("tsel_no_separator", "tsel 1 \"x\"y\n"),
    ("tsel_no_text", "tsel 1\n"),
    ("tsel_trailing_space", "tsel 1 \n"),
    ("tsel_bad_line", "tsel x \"y\"\n"),
    ("tsel_escaped", "tsel 1 \"a\\nb\" 2\n"),
    ("tsel_tab", "tsel 1 \"a\tb\"\n"),
    ("tsel_physical_newline", "tsel 1 \"a\nb\"\n"),
    # --- bsel ---------------------------------------------------------------
    ("bsel_simple", "bsel \"start\" \"end\"\n"),
    ("bsel_empty_start", "bsel \"\" \"x\"\n"),
    ("bsel_empty_end", "bsel \"x\" \"\"\n"),
    ("bsel_same", "bsel \"x\" \"x\"\n"),
    ("bsel_no_separator", "bsel \"a\"\"b\"\n"),
    ("bsel_trailing", "bsel \"a\" \"b\" c\n"),
    ("bsel_unquoted", "bsel a \"b\"\n"),
    ("bsel_bare", "bsel\n"),
    ("bsel_unicode", "bsel \"bắt đầu\" \"kết thúc\"\n"),
    # --- rsel ---------------------------------------------------------------
    ("rsel_simple", "rsel 2:4\n"),
    ("rsel_reversed", "rsel 4:2\n"),
    ("rsel_zero", "rsel 0:2\n"),
    ("rsel_leading_zero", "rsel 02:2\n"),
    ("rsel_bad", "rsel 2:x\n"),
    ("rsel_trailing", "rsel 2:4 5\n"),
    ("rsel_huge", "rsel 99999999999999999999:2\n"),
    ("rsel_no_colon", "rsel 24\n"),
    # --- type ---------------------------------------------------------------
    ("type_simple", "type \"hello\"\n"),
    ("type_empty", "type \"\"\n"),
    ("type_escapes", "type \"a\\nb\\t\\\"q\\\\\"\n"),
    ("type_unicode", "type \"héllo 世界\"\n"),
    ("type_tab", "type \"a\tb\"\n"),
    ("type_trailing", "type \"a\" b\n"),
    ("type_unquoted", "type x\n"),
    ("type_unterminated", "type \"abc\n"),
    ("type_bad_escape", "type \"a\\qb\"\n"),
    ("type_raw_control", "type \"a\x01b\"\n"),
    ("type_u_escape", "type \"\\u0041\\u00e9\"\n"),
    ("type_u_surrogate", "type \"\\ud83d\\ude00\"\n"),
    ("type_u_lone", "type \"\\ud800\"\n"),
    ("type_u_bad_hex", "type \"\\u12G4\"\n"),
    ("type_u_truncated", "type \"\\u12\"\n"),
    ("type_physical_newline", "type \"a\nb\"\n"),
    ("type_bare", "type\n"),
    ("type_space_only", "type \n"),
    ("bsel_space_only", "bsel \n"),
    ("type_u_high_then_ascii", "type \"\\ud800\\u0041\"\n"),
    ("type_u_low_lone", "type \"\\udc00\"\n"),
    ("type_no_space", "type\"x\"\n"),
    # --- heredoc ------------------------------------------------------------
    ("heredoc_lf", "type <<EOF\na\nb\nEOF\n"),
    ("heredoc_crlf", "type <<EOF\r\na\r\nb\r\nEOF\r\n"),
    ("heredoc_quoted_delim", "type <<'EOF'\na\nEOF\n"),
    ("heredoc_double_quoted_delim", "type <<\"EOF\"\na\nEOF\n"),
    ("heredoc_empty_body", "type <<EOF\nEOF\n"),
    ("heredoc_body_like_delim", "type <<EOF\nEOF\nEOF\n"),
    ("heredoc_delim_64", "type <<" + "a" * 64 + "\nx\n" + "a" * 64 + "\n"),
    ("heredoc_delim_too_long", "type <<" + "a" * 65 + "\nx\n"),
    ("heredoc_delim_invalid", "type <<EOF!\nx\n"),
    ("heredoc_delim_empty", "type <<\nx\n"),
    ("heredoc_delim_mismatched_quotes", "type <<'EOF\"\nx\n"),
    ("heredoc_unterminated", "type <<EOF\na\nb\n"),
    ("heredoc_after_commands", "in a.txt\ntype <<EOF\nx\nEOF\nrm\n"),
    ("heredoc_unknown_after", "type <<EOF\nx\nEOF\nunknown_command\n"),
]

DYNAMIC_SCRIPTS = {
    "heredoc_oversized": '"type <<EOF\\n" + strings.Repeat("x", 1<<20) + "\\nEOF\\n"',
}

GO_TEST_TEMPLATE = '''package apx

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type expectedInstruction struct {
	Line       int    `json:"line"`
	Operation  string `json:"operation"`
	Path       string `json:"path"`
	LineNumber int    `json:"lineNumber"`
	EndLine    int    `json:"endLine"`
	Start      int    `json:"start"`
	End        int    `json:"end"`
	Count      int    `json:"count"`
	Text       string `json:"text"`
	EndText    string `json:"endText"`
}

type expectedError struct {
	Line       int    `json:"line"`
	Command    int    `json:"command"`
	Operation  string `json:"operation"`
	Category   string `json:"category"`
	Message    string `json:"message"`
	Diagnostic string `json:"diagnostic"`
}

type expectedResult struct {
	Instructions []expectedInstruction `json:"instructions"`
	Errors       []expectedError       `json:"errors"`
}

type fixtureCase struct {
	Name     string         `json:"name"`
	Script   string         `json:"script"`
	Expected expectedResult `json:"expected"`
}

type fixtureCorpus struct {
	GoOracleRevision string        `json:"go_oracle_revision"`
	Cases            []fixtureCase `json:"cases"`
}

func TestGenerateFixtures(t *testing.T) {
	outputPath := os.Getenv("APX_FIXTURE_OUTPUT_PATH")
	if outputPath == "" {
		t.Fatal("APX_FIXTURE_OUTPUT_PATH environment variable is not set")
	}

	cases := []struct {
		name   string
		script string
	}{
__CASES__
	}

	var fixtureCases []fixtureCase
	for _, tc := range cases {
		prog, err := parse(tc.script)
		var res expectedResult
		if err != nil {
			errs := commandsOf(err)
			for _, e := range errs {
				res.Errors = append(res.Errors, expectedError{
					Line:       e.Line,
					Command:    e.Command,
					Operation:  e.Operation,
					Category:   e.Category,
					Message:    e.Message,
					Diagnostic: failureDiagnostic(e.Error()),
				})
			}
		} else {
			for _, inst := range prog.instructions {
				res.Instructions = append(res.Instructions, expectedInstruction{
					Line:       inst.line,
					Operation:  inst.operation,
					Path:       inst.path,
					LineNumber: inst.lineNumber,
					EndLine:    inst.endLine,
					Start:      inst.start,
					End:        inst.end,
					Count:      inst.count,
					Text:       inst.text,
					EndText:    inst.endText,
				})
			}
		}
		fixtureCases = append(fixtureCases, fixtureCase{
			Name:     tc.name,
			Script:   tc.script,
			Expected: res,
		})
	}

	corpus := fixtureCorpus{
		GoOracleRevision: "__REVISION__",
		Cases:            fixtureCases,
	}
	if err := os.MkdirAll(filepath.Dir(outputPath), 0755); err != nil {
		t.Fatalf("failed to create directory: %v", err)
	}
	data, err := json.MarshalIndent(corpus, "", "  ")
	if err != nil {
		t.Fatalf("failed to marshal json: %v", err)
	}
	if err := os.WriteFile(outputPath, data, 0644); err != nil {
		t.Fatalf("failed to write file: %v", err)
	}
}
'''


def get_rust_repo_path():
    return Path(__file__).resolve().parent.parent


def go_quote(value):
    out = ['"']
    for character in value:
        if character == '"':
            out.append('\\"')
        elif character == '\\':
            out.append('\\\\')
        elif character == '\n':
            out.append('\\n')
        elif character == '\r':
            out.append('\\r')
        elif character == '\t':
            out.append('\\t')
        elif ord(character) < 0x20 or ord(character) == 0x7F:
            out.append('\\x%02x' % ord(character))
        else:
            out.append(character)
    out.append('"')
    return ''.join(out)


def go_test_source():
    rendered = []
    entries = list(CASES) + [
        (name, "") for name in DYNAMIC_SCRIPTS if name not in {n for n, _ in CASES}
    ]
    for name, script in entries:
        if name in DYNAMIC_SCRIPTS:
            script_expr = DYNAMIC_SCRIPTS[name]
        else:
            script_expr = go_quote(script)
        rendered.append("\t\t{name: %s, script: %s}," % (go_quote(name), script_expr))
    return (
        GO_TEST_TEMPLATE.replace("__CASES__", "\n".join(rendered))
        .replace("__REVISION__", REQUIRED_SHA)
    )


def _generate_in_repo(repo_dir, output_path):
    target = Path(repo_dir) / TEST_FILENAME
    created = False
    try:
        descriptor = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    except FileExistsError:
        print(f"Error: A test file already exists at {target}. Refusing to overwrite.", file=sys.stderr)
        sys.exit(1)
    created = True
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as f:
            f.write(go_test_source())
        env = os.environ.copy()
        env["APX_FIXTURE_OUTPUT_PATH"] = str(output_path)
        result = subprocess.run(
            ["go", "test", "-run", "TestGenerateFixtures"],
            cwd=repo_dir,
            env=env,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print("Go test failed:", result.stderr, file=sys.stderr)
            sys.exit(result.returncode)
    finally:
        if created and target.exists():
            target.unlink()


def generate_fixtures(go_repo_path, output_path):
    go_repo_path = Path(go_repo_path)
    with tempfile.TemporaryDirectory(prefix="apx-oracle-") as temporary:
        worktree = Path(temporary) / "worktree"
        try:
            subprocess.run(
                ["git", "worktree", "add", "--detach", str(worktree), REQUIRED_SHA],
                cwd=go_repo_path,
                capture_output=True,
                text=True,
                check=True,
            )
        except subprocess.CalledProcessError as error:
            print(
                f"Error: cannot check out oracle {REQUIRED_SHA} from {go_repo_path}: {error.stderr}",
                file=sys.stderr,
            )
            sys.exit(1)
        try:
            _generate_in_repo(worktree, output_path)
        finally:
            subprocess.run(
                ["git", "worktree", "remove", "--force", str(worktree)],
                cwd=go_repo_path,
                capture_output=True,
            )


def run_safety_tests(go_repo_path):
    print("Running parity.py generator safety tests...")
    with tempfile.TemporaryDirectory() as temporary:
        copied_repo = Path(temporary) / "apx"
        shutil.copytree(go_repo_path, copied_repo)
        target = copied_repo / TEST_FILENAME
        target.write_text("sentinel", encoding="utf-8")
        print("Pre-existing file exists. Trying to generate (should fail)...")
        try:
            _generate_in_repo(copied_repo, Path(temporary) / "parser.json")
            print("FAILED: Pre-existing file check did not abort.")
            sys.exit(1)
        except SystemExit as error:
            if error.code == 1:
                print("PASSED: Aborted correctly on pre-existing file.")
            else:
                raise error
        if target.read_text(encoding="utf-8") != "sentinel":
            print("FAILED: Pre-existing file was modified or removed.", file=sys.stderr)
            sys.exit(1)
    print("Safety tests completed successfully.")


def main():
    rust_repo = get_rust_repo_path()
    sibling_go_repo = rust_repo.parent / "apx"

    argument_parser = argparse.ArgumentParser(description="Fixtures Parity Generator/Verifier")
    argument_parser.add_argument(
        "--go-repo",
        type=str,
        default=str(sibling_go_repo),
        help="Path to Go oracle repository",
    )
    argument_parser.add_argument(
        "command",
        choices=["generate", "verify", "verify-rust", "test-safety"],
        help="Command to run",
    )
    args = argument_parser.parse_args()

    go_repo_path = Path(args.go_repo).resolve()
    fixture_path = rust_repo / "fixtures/corpus/parser.json"

    if args.command == "generate":
        print(f"Generating fixtures to {fixture_path}...")
        generate_fixtures(go_repo_path, fixture_path)
        print("Generation completed successfully.")
    elif args.command == "verify":
        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as temporary:
            temporary_path = Path(temporary.name)
        try:
            generate_fixtures(go_repo_path, temporary_path)
            content1 = fixture_path.read_text(encoding="utf-8")
            content2 = temporary_path.read_text(encoding="utf-8")
            if content1 != content2:
                print("Fixture mismatch detected!", file=sys.stderr)
                diff = difflib.unified_diff(
                    content1.splitlines(keepends=True),
                    content2.splitlines(keepends=True),
                    fromfile="fixtures/corpus/parser.json",
                    tofile="generated",
                )
                sys.stdout.writelines(diff)
                sys.exit(1)
            print("Fixtures match Go oracle output.")
        finally:
            if temporary_path.exists():
                temporary_path.unlink()
    elif args.command == "verify-rust":
        result = subprocess.run(
            ["cargo", "test", "-p", "apx-core", "--test", "corpus_parity"],
            cwd=rust_repo,
            capture_output=True,
            text=True,
        )
        sys.stdout.write(result.stdout)
        if result.returncode != 0:
            sys.stderr.write(result.stderr)
            sys.exit(result.returncode)
        print("Rust differential test passed.")
    elif args.command == "test-safety":
        run_safety_tests(go_repo_path)


if __name__ == "__main__":
    main()
