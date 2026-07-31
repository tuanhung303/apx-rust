#!/usr/bin/env python3
import os
import sys
import subprocess
import tempfile
import shutil
import json
import argparse
import difflib
from pathlib import Path

REQUIRED_SHA = "bcd85fc2f817e7c405f8b92953cd3ad4db165759"
TEST_FILENAME = "fixtures_generation_test.go"

TEST_FILE_CONTENT = """package apx

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

type expectedInstruction struct {
	Line      int    `json:"line"`
	Operation string `json:"operation"`
	Path      string `json:"path"`
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
		{
			name:   "in_simple",
			script: "in file.txt\\n",
		},
		{
			name:   "new_simple",
			script: "new file.txt\\n",
		},
		{
			name:   "mv_simple",
			script: "mv file.txt\\n",
		},
		{
			name:   "rm_simple",
			script: "rm file.txt\\n",
		},
		{
			name:   "rm_no_path",
			script: "rm\\n",
		},
		{
			name:   "in_no_space",
			script: "in\\n",
		},
		{
			name:   "new_no_space",
			script: "new\\n",
		},
		{
			name:   "mv_no_space",
			script: "mv\\n",
		},
		{
			name:   "in_empty_path",
			script: "in \\n",
		},
		{
			name:   "new_empty_path",
			script: "new \\n",
		},
		{
			name:   "mv_empty_path",
			script: "mv \\n",
		},
		{
			name:   "rm_empty_path",
			script: "rm \\n",
		},
		{
			name:   "path_cleaning",
			script: "in a/b/../c/./d\\n",
		},
		{
			name:   "path_spaces",
			script: "in a b.txt\\n",
		},
		{
			name:   "multiple_commands",
			script: "in file.txt\\nrm\\nnew other.txt\\n",
		},
		{
			name:   "empty_and_whitespace_lines",
			script: "\\n  \\nin file.txt\\n\\n\\t\\nrm\\n",
		},
		{
			name:   "unknown_command",
			script: "unknown_command\\n",
		},
		{
			name:   "multiple_errors",
			script: "in \\nunknown_command\\nnew \\n",
		},
		{
			name:   "crlf_line_endings",
			script: "in file.txt\\r\\nrm\\r\\n",
		},
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
					Line:      inst.line,
					Operation: inst.operation,
					Path:      inst.path,
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
		GoOracleRevision: "bcd85fc2f817e7c405f8b92953cd3ad4db165759",
		Cases:            fixtureCases,
	}

	err := os.MkdirAll(filepath.Dir(outputPath), 0755)
	if err != nil {
		t.Fatalf("failed to create directory: %v", err)
	}

	data, err := json.MarshalIndent(corpus, "", "  ")
	if err != nil {
		t.Fatalf("failed to marshal json: %v", err)
	}

	err = os.WriteFile(outputPath, data, 0644)
	if err != nil {
		t.Fatalf("failed to write file: %v", err)
	}
}
"""

def get_rust_repo_path():
    return Path(__file__).resolve().parent.parent

def check_go_repo_head(go_repo_path):
    try:
        res = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=go_repo_path,
            capture_output=True,
            text=True,
            check=True
        )
        sha = res.stdout.strip()
        if sha != REQUIRED_SHA:
            print(f"Error: Go repository HEAD is {sha}, but {REQUIRED_SHA} is required.", file=sys.stderr)
            sys.exit(1)
    except subprocess.CalledProcessError as e:
        print(f"Error checking git HEAD in {go_repo_path}: {e}", file=sys.stderr)
        sys.exit(1)

def generate_fixtures(go_repo_path, output_path):
    target_test_file = go_repo_path / TEST_FILENAME
    created_test_file = False
    
    # 1. Verify safety: require Go repo HEAD to equal bcd85fc2f817e7c405f8b92953cd3ad4db165759
    check_go_repo_head(go_repo_path)
    
    try:
        try:
            descriptor = os.open(
                target_test_file,
                os.O_WRONLY | os.O_CREAT | os.O_EXCL,
                0o644,
            )
        except FileExistsError:
            print(f"Error: A test file already exists at {target_test_file}. Refusing to overwrite.", file=sys.stderr)
            sys.exit(1)
        created_test_file = True
        with os.fdopen(descriptor, "w", encoding="utf-8") as f:
            f.write(TEST_FILE_CONTENT)
        
        env = os.environ.copy()
        env["APX_FIXTURE_OUTPUT_PATH"] = str(output_path)
        
        res = subprocess.run(
            ["go", "test", "-run", "TestGenerateFixtures"],
            cwd=go_repo_path,
            env=env,
            capture_output=True,
            text=True
        )
        if res.returncode != 0:
            print("Go test failed:", res.stderr, file=sys.stderr)
            sys.exit(res.returncode)
    finally:
        if created_test_file:
            target_test_file.unlink()

def run_safety_tests(go_repo_path):
    print("Running parity.py generator safety tests...")
    with tempfile.TemporaryDirectory() as temporary:
        copied_repo = Path(temporary) / "apx"
        shutil.copytree(go_repo_path, copied_repo)
        target_test_file = copied_repo / TEST_FILENAME
        target_test_file.write_text("sentinel", encoding="utf-8")
        print("Pre-existing file exists. Trying to generate (should fail)...")
        try:
            generate_fixtures(copied_repo, Path(temporary) / "parser.json")
            print("FAILED: Pre-existing file check did not abort.")
            sys.exit(1)
        except SystemExit as e:
            if e.code == 1:
                print("PASSED: Aborted correctly on pre-existing file.")
            else:
                raise e
        if target_test_file.read_text(encoding="utf-8") != "sentinel":
            print("FAILED: Pre-existing file was modified or removed.", file=sys.stderr)
            sys.exit(1)
    print("Safety tests completed successfully.")

def main():
    rust_repo = get_rust_repo_path()
    sibling_go_repo = rust_repo.parent / "apx"
    
    parser = argparse.ArgumentParser(description="Fixtures Parity Generator/Verifier")
    parser.add_argument("--go-repo", type=str, default=str(sibling_go_repo), help="Path to Go oracle repository")
    parser.add_argument("command", choices=["generate", "verify", "test-safety"], help="Command to run")
    
    args = parser.parse_args()
    
    go_repo_path = Path(args.go_repo).resolve()
    fixture_path = rust_repo / "fixtures/corpus/parser.json"
    
    if args.command == "generate":
        print(f"Generating fixtures to {fixture_path}...")
        generate_fixtures(go_repo_path, fixture_path)
        print("Generation completed successfully.")
    elif args.command == "verify":
        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tmp:
            tmp_path = Path(tmp.name)
        try:
            generate_fixtures(go_repo_path, tmp_path)
            with open(fixture_path, "r", encoding="utf-8") as f1, open(tmp_path, "r", encoding="utf-8") as f2:
                content1 = f1.read()
                content2 = f2.read()
            if content1 != content2:
                print("Fixture mismatch detected!", file=sys.stderr)
                # Diff output using difflib
                diff = difflib.unified_diff(
                    content1.splitlines(keepends=True),
                    content2.splitlines(keepends=True),
                    fromfile="fixtures/corpus/parser.json",
                    tofile="generated"
                )
                sys.stdout.writelines(diff)
                sys.exit(1)
            else:
                print("Fixtures match Go oracle output.")
                sys.exit(0)
        finally:
            if tmp_path.exists():
                tmp_path.unlink()
    elif args.command == "test-safety":
        run_safety_tests(go_repo_path)

if __name__ == "__main__":
    main()
