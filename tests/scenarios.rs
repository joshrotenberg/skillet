//! Multi-step workflow scenario tests.
//!
//! Each test chains multiple CLI operations to exercise real user workflows
//! end-to-end, verifying that outputs from one step are valid inputs to the next.
//!
//! All tests use `tempfile` for isolation and override `$HOME` so nothing
//! touches the real filesystem.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn test_registry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-registry")
}

fn test_npm_registry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-npm-registry")
}

fn official_registry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("registry")
}

#[allow(deprecated)]
fn skillet() -> Command {
    Command::cargo_bin("skillet").expect("binary exists")
}

// ── Author flow ──────────────────────────────────────────────────────

/// init-skill -> write content -> validate -> pack -> verify artifacts
#[test]
fn scenario_author_flow() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("testauthor/my-skill");

    // Step 1: Scaffold
    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .args([
            "--description",
            "A workflow test skill",
            "--category",
            "testing",
        ])
        .assert()
        .success();

    // Step 2: Write real content into SKILL.md
    std::fs::write(
        skill_path.join("SKILL.md"),
        "# My Skill\n\nThis skill helps with workflow testing.\n\n## Usage\n\nJust use it.\n",
    )
    .expect("write SKILL.md");

    // Step 3: Validate
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Validation passed")
                .and(predicate::str::contains("testauthor"))
                .and(predicate::str::contains("my-skill")),
        );

    // Step 4: Pack
    skillet()
        .args(["pack"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pack succeeded"));

    // Step 5: Verify artifacts
    assert!(skill_path.join("MANIFEST.sha256").exists());
    assert!(skill_path.join("versions.toml").exists());

    let manifest =
        std::fs::read_to_string(skill_path.join("MANIFEST.sha256")).expect("read manifest");
    assert!(
        manifest.contains("SKILL.md"),
        "manifest should reference SKILL.md: {manifest}"
    );
    assert!(
        manifest.contains("skill.toml"),
        "manifest should reference skill.toml: {manifest}"
    );

    let versions =
        std::fs::read_to_string(skill_path.join("versions.toml")).expect("read versions");
    assert!(
        versions.contains("[[versions]]"),
        "versions.toml should have version entries: {versions}"
    );

    // Step 6: Pack again should be idempotent
    skillet()
        .args(["pack"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pack succeeded"));

    // Step 7: Validate the packed skill still passes
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Validation passed"));
}

// ── Consumer flow ────────────────────────────────────────────────────

/// search -> info -> install -> list -> verify files on disk
#[test]
fn scenario_consumer_flow() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Step 1: Search
    let search_output = skillet()
        .args(["search", "rust", "--registry"])
        .arg(test_registry())
        .output()
        .expect("search");
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("joshrotenberg/rust-dev"),
        "search should find rust-dev: {search_stdout}"
    );

    // Step 2: Info on the found skill
    skillet()
        .args(["info", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("version")
                .and(predicate::str::contains("description"))
                .and(predicate::str::contains("categories")),
        );

    // Step 3: Install
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed joshrotenberg/rust-dev"));

    // Step 4: List shows it
    skillet()
        .args(["list"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("joshrotenberg/rust-dev"));

    // Step 5: Verify file content matches registry
    let installed_md = std::fs::read_to_string(tmp.path().join(".agents/skills/rust-dev/SKILL.md"))
        .expect("read installed SKILL.md");
    let registry_md =
        std::fs::read_to_string(test_registry().join("joshrotenberg/rust-dev/SKILL.md"))
            .expect("read registry SKILL.md");
    assert_eq!(
        installed_md, registry_md,
        "installed SKILL.md should match registry content"
    );
}

// ── Multi-registry merge ─────────────────────────────────────────────

/// Two registries with overlapping names: first-registry-wins on collision,
/// unique skills from second registry still present.
#[test]
fn scenario_multi_registry_merge() {
    let tmp = tempfile::tempdir().expect("create temp dir");

    // Create two mini registries
    let reg_a = tmp.path().join("reg-a");
    let reg_b = tmp.path().join("reg-b");

    // Registry A: has "shared/tool" and "alpha/unique-a"
    create_mini_skill(&reg_a, "shared", "tool", "Tool from registry A");
    create_mini_skill(&reg_a, "alpha", "unique-a", "Only in A");
    write_registry_config(&reg_a, "Registry A");

    // Registry B: has "shared/tool" (different description) and "beta/unique-b"
    create_mini_skill(&reg_b, "shared", "tool", "Tool from registry B");
    create_mini_skill(&reg_b, "beta", "unique-b", "Only in B");
    write_registry_config(&reg_b, "Registry B");

    // Search with both registries (A first)
    let output = skillet()
        .args(["search", "*", "--registry"])
        .arg(&reg_a)
        .args(["--registry"])
        .arg(&reg_b)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // First registry wins for shared/tool
    assert!(
        stdout.contains("Tool from registry A"),
        "first registry should win for shared/tool: {stdout}"
    );
    assert!(
        !stdout.contains("Tool from registry B"),
        "second registry description should not appear: {stdout}"
    );

    // Unique skills from both registries present
    assert!(
        stdout.contains("unique-a"),
        "unique-a from reg A should appear: {stdout}"
    );
    assert!(
        stdout.contains("unique-b"),
        "unique-b from reg B should appear: {stdout}"
    );
}

// ── Trust flow ───────────────────────────────────────────────────────

/// install -> verify auto-pin -> audit ok -> modify file -> audit detects mismatch
#[test]
fn scenario_trust_flow() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Step 1: Install (auto-pin is enabled by default)
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    // Step 2: Verify trust state was written (auto-pin)
    let trust_path = home.join(".config/skillet/trust.toml");
    assert!(
        trust_path.exists(),
        "trust.toml should exist after install: {}",
        trust_path.display()
    );
    let trust_content = std::fs::read_to_string(&trust_path).expect("read trust.toml");
    assert!(
        trust_content.contains("rust-dev"),
        "trust state should contain pinned skill: {trust_content}"
    );

    // Step 3: Audit should pass
    skillet()
        .args(["audit"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[ok]"));

    // Step 4: Modify the installed SKILL.md
    let installed_path = tmp.path().join(".agents/skills/rust-dev/SKILL.md");
    let mut content = std::fs::read_to_string(&installed_path).expect("read SKILL.md");
    content.push_str("\n<!-- tampered -->\n");
    std::fs::write(&installed_path, content).expect("write modified SKILL.md");

    // Step 5: Audit should detect mismatch
    skillet()
        .args(["audit"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("[MODIFIED]"));
}

// ── Safety flow ──────────────────────────────────────────────────────

/// Create unsafe skill -> validate fails with exit 2 -> fix it -> validate passes
#[test]
fn scenario_safety_flow() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("testauthor/risky-skill");

    // Step 1: Scaffold
    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .assert()
        .success();

    // Step 2: Write dangerous content
    std::fs::write(
        skill_path.join("SKILL.md"),
        "# Risky Skill\n\nRun this: $(rm -rf /)\nAlso: eval \"$USER_INPUT\"\n",
    )
    .expect("write dangerous SKILL.md");

    // Step 3: Validate should fail with exit code 2
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .code(2)
        .stdout(predicate::str::contains("Safety scan"))
        .stderr(predicate::str::contains("safety issues detected"));

    // Step 4: Fix the content
    std::fs::write(
        skill_path.join("SKILL.md"),
        "# Safe Skill\n\nThis skill is perfectly safe and helpful.\n",
    )
    .expect("write safe SKILL.md");

    // Step 5: Validate should now pass
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Validation passed"));
}

// ── Install multiple targets ─────────────────────────────────────────

/// Install to multiple targets, verify each gets the files
#[test]
fn scenario_install_multiple_targets() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Install to agents and claude
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents", "--target", "claude"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed joshrotenberg/rust-dev"));

    // Both target directories should have SKILL.md
    assert!(
        tmp.path().join(".agents/skills/rust-dev/SKILL.md").exists(),
        "agents target should have SKILL.md"
    );
    assert!(
        tmp.path().join(".claude/skills/rust-dev/SKILL.md").exists(),
        "claude target should have SKILL.md"
    );
}

// ── Author with extra files ──────────────────────────────────────────

/// init-skill -> add scripts + references -> validate -> pack -> verify manifest includes them
#[test]
fn scenario_author_with_extra_files() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("testauthor/full-skill");

    // Step 1: Scaffold
    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .args(["--description", "A skill with extra files"])
        .assert()
        .success();

    // Step 2: Add extra files
    std::fs::create_dir_all(skill_path.join("scripts")).expect("create scripts dir");
    std::fs::write(
        skill_path.join("scripts/setup.sh"),
        "#!/bin/bash\necho 'setup'\n",
    )
    .expect("write script");

    std::fs::create_dir_all(skill_path.join("references")).expect("create references dir");
    std::fs::write(
        skill_path.join("references/GUIDE.md"),
        "# Guide\n\nSome reference docs.\n",
    )
    .expect("write reference");

    // Step 3: Validate (should mention extra files)
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Validation passed")
                .and(predicate::str::contains("scripts/setup.sh")),
        );

    // Step 4: Pack
    skillet()
        .args(["pack"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pack succeeded"));

    // Step 5: Manifest should include all files
    let manifest =
        std::fs::read_to_string(skill_path.join("MANIFEST.sha256")).expect("read manifest");
    assert!(
        manifest.contains("scripts/setup.sh"),
        "manifest should include script: {manifest}"
    );
    assert!(
        manifest.contains("references/GUIDE.md"),
        "manifest should include reference: {manifest}"
    );
}

// ── Install with extra files ─────────────────────────────────────────

/// Install a skill with extra files and verify they're written to disk
#[test]
fn scenario_install_with_extra_files() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // python-dev has scripts/ and references/
    skillet()
        .args(["install", "acme/python-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed acme/python-dev"));

    let skill_dir = tmp.path().join(".agents/skills/python-dev");
    assert!(skill_dir.join("SKILL.md").exists(), "SKILL.md should exist");
    assert!(
        skill_dir.join("scripts/lint.sh").exists(),
        "scripts/lint.sh should exist"
    );
    assert!(
        skill_dir.join("references/RUFF_CONFIG.md").exists(),
        "references/RUFF_CONFIG.md should exist"
    );
}

// ── Reinstall overwrites ─────────────────────────────────────────────

/// Install, modify installed file, reinstall, verify original content restored
#[test]
fn scenario_reinstall_overwrites() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Install
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    let installed_path = tmp.path().join(".agents/skills/rust-dev/SKILL.md");
    let original = std::fs::read_to_string(&installed_path).expect("read original");

    // Modify
    std::fs::write(&installed_path, "tampered content").expect("tamper");

    // Reinstall
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    // Verify original content restored
    let after_reinstall = std::fs::read_to_string(&installed_path).expect("read after reinstall");
    assert_eq!(
        original, after_reinstall,
        "reinstall should restore original content"
    );
}

// ── Nested registry skills ───────────────────────────────────────────

/// search -> info -> install -> list with nested registry structure
#[test]
fn scenario_nested_registry_skills() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Step 1: Search should find nested skills
    let search_output = skillet()
        .args(["search", "maven", "--registry"])
        .arg(test_registry())
        .output()
        .expect("search");
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("acme/maven-build"),
        "search should find nested maven-build: {search_stdout}"
    );

    // Step 2: Info on the nested skill should show registry path
    skillet()
        .args(["info", "acme/maven-build", "--registry"])
        .arg(test_registry())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("version")
                .and(predicate::str::contains("description"))
                .and(predicate::str::contains("registry path"))
                .and(predicate::str::contains("acme/lang/java/maven-build")),
        );

    // Step 3: Install the nested skill
    skillet()
        .args(["install", "acme/maven-build", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed acme/maven-build"));

    // Step 4: List shows it
    skillet()
        .args(["list"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("acme/maven-build"));

    // Step 5: Verify installed content matches source
    let installed_md =
        std::fs::read_to_string(tmp.path().join(".agents/skills/maven-build/SKILL.md"))
            .expect("read installed SKILL.md");
    let registry_md =
        std::fs::read_to_string(test_registry().join("acme/lang/java/maven-build/SKILL.md"))
            .expect("read registry SKILL.md");
    assert_eq!(
        installed_md, registry_md,
        "installed SKILL.md should match registry content"
    );
}

// ── Helpers ──────────────────────────────────────────────────────────

fn create_mini_skill(registry: &std::path::Path, owner: &str, name: &str, description: &str) {
    let skill_dir = registry.join(owner).join(name);
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");

    let toml = format!(
        "[skill]\nname = \"{name}\"\nowner = \"{owner}\"\nversion = \"1.0.0\"\ndescription = \"{description}\"\n"
    );
    std::fs::write(skill_dir.join("skill.toml"), toml).expect("write skill.toml");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("# {name}\n\n{description}\n"),
    )
    .expect("write SKILL.md");
}

fn write_registry_config(registry: &std::path::Path, name: &str) {
    let config = format!("[registry]\nname = \"{name}\"\n");
    std::fs::write(registry.join("skillet.toml"), config).expect("write skillet.toml");
}

// ── Publish workflow (#143) ──────────────────────────────────────

/// init-skill -> write content -> pack -> publish --dry-run -> verify output
#[test]
fn scenario_publish_dry_run() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("testauthor/publishable-skill");

    // Step 1: Scaffold
    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .args([
            "--description",
            "A skill ready to publish",
            "--category",
            "development",
        ])
        .assert()
        .success();

    // Step 2: Write real content
    std::fs::write(
        skill_path.join("SKILL.md"),
        "# Publishable Skill\n\nThis skill is ready for publishing.\n\n## Usage\n\nJust use it.\n",
    )
    .expect("write SKILL.md");

    // Step 3: Validate first
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Validation passed"));

    // Step 4: Publish with --dry-run (doesn't need gh CLI)
    skillet()
        .args(["publish"])
        .arg(&skill_path)
        .args(["--repo", "testowner/test-registry", "--dry-run"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Dry run")
                .and(predicate::str::contains("testauthor/publishable-skill"))
                .and(predicate::str::contains("testowner/test-registry"))
                .and(predicate::str::contains("Fork"))
                .and(predicate::str::contains("branch"))
                .and(predicate::str::contains("PR")),
        );

    // Step 5: Pack artifacts should have been created by publish (it calls pack internally)
    assert!(
        skill_path.join("MANIFEST.sha256").exists(),
        "MANIFEST.sha256 should be created by publish --dry-run"
    );
    assert!(
        skill_path.join("versions.toml").exists(),
        "versions.toml should be created by publish --dry-run"
    );
}

/// publish --dry-run with --registry-path overrides destination
#[test]
fn scenario_publish_dry_run_custom_path() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("testauthor/nested-skill");

    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .assert()
        .success();

    skillet()
        .args(["publish"])
        .arg(&skill_path)
        .args([
            "--repo",
            "owner/repo",
            "--dry-run",
            "--registry-path",
            "acme/lang/java/nested-skill",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("acme/lang/java/nested-skill"));
}

// ── Project manifest lifecycle (#144) ────────────────────────────
//
// Local discovery (#144) only runs in MCP server context, not CLI.
// These tests exercise project manifest features testable via CLI.

/// init-project -> add skills -> search finds them via --registry
#[test]
fn scenario_project_manifest_as_registry() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let project = tmp.path().join("my-project");
    std::fs::create_dir_all(&project).expect("create project dir");

    // Step 1: Create a project with a skills directory
    let skills_dir = project.join("skills");
    std::fs::create_dir_all(&skills_dir).expect("create skills dir");

    // Write skillet.toml with [skills] pointing to skills/
    std::fs::write(
        project.join("skillet.toml"),
        "[project]\nname = \"my-project\"\ndescription = \"Test project\"\n\n[skills]\npath = \"skills\"\n",
    )
    .expect("write skillet.toml");

    // Step 2: Add a skill in skills/
    let skill_dir = skills_dir.join("my-tool");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "# My Tool\n\nA project-embedded skill for testing.\n",
    )
    .expect("write SKILL.md");

    // Step 3: Search with --registry pointing at the project
    skillet()
        .args(["search", "*", "--registry"])
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("my-tool"));
}

/// init-project scaffolding creates correct structure
#[test]
fn scenario_init_project_lifecycle() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let project = tmp.path().join("new-project");

    // Step 1: Init project with --skill
    skillet()
        .args(["init-project"])
        .arg(&project)
        .args(["--skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skillet.toml"));

    // Step 2: Verify structure
    assert!(
        project.join("skillet.toml").exists(),
        "skillet.toml should exist"
    );
    assert!(
        project.join("SKILL.md").exists(),
        "SKILL.md should exist for --skill"
    );

    // Step 3: skillet.toml should have [skill] section
    let toml = std::fs::read_to_string(project.join("skillet.toml")).expect("read skillet.toml");
    assert!(
        toml.contains("[skill]"),
        "should have [skill] section: {toml}"
    );
    assert!(
        toml.contains("[project]"),
        "should have [project] section: {toml}"
    );

    // Step 4: Search with --registry should find the embedded skill
    skillet()
        .args(["search", "*", "--registry"])
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("new-project"));
}

/// init-project with --multi creates multi-skill directory
#[test]
fn scenario_init_project_multi() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let project = tmp.path().join("multi-project");

    // Step 1: Init with --multi
    skillet()
        .args(["init-project"])
        .arg(&project)
        .args(["--multi"])
        .assert()
        .success();

    // Step 2: Verify structure
    let toml = std::fs::read_to_string(project.join("skillet.toml")).expect("read skillet.toml");
    assert!(
        toml.contains("[skills]"),
        "should have [skills] section: {toml}"
    );

    // Step 3: Add skills to .skillet/ directory
    let skill_dir = project.join(".skillet/helper");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(skill_dir.join("SKILL.md"), "# Helper\n\nA helper skill.\n")
        .expect("write SKILL.md");

    // Step 4: Search should find it
    skillet()
        .args(["search", "*", "--registry"])
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("helper"));
}

// ── Config and setup workflow (#145) ─────────────────────────────

/// setup -> verify config -> customize -> verify effects
#[test]
fn scenario_config_lifecycle() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Step 1: Run setup
    skillet()
        .args(["setup", "--target", "claude", "--no-official-registry"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("config.toml"));

    let config_path = home.join(".config/skillet/config.toml");
    assert!(config_path.exists(), "config.toml should exist");

    let config = std::fs::read_to_string(&config_path).expect("read config");
    assert!(
        config.contains("claude"),
        "config should contain claude target: {config}"
    );

    // Step 2: Install with the config (should use claude target from config)
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    // Should install to .claude/ (from config target), not .agents/
    assert!(
        tmp.path().join(".claude/skills/rust-dev/SKILL.md").exists(),
        "should install to .claude/ per config"
    );

    // Step 3: setup --force regenerates config
    skillet()
        .args([
            "setup",
            "--target",
            "agents",
            "--force",
            "--no-official-registry",
        ])
        .env("HOME", &home)
        .assert()
        .success();

    let updated_config = std::fs::read_to_string(&config_path).expect("read updated config");
    assert!(
        updated_config.contains("agents"),
        "regenerated config should contain agents target: {updated_config}"
    );
}

/// Setup with custom registry, verify it appears in config
#[test]
fn scenario_setup_with_registry() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["setup", "--registry"])
        .arg(test_registry())
        .args(["--no-official-registry"])
        .env("HOME", &home)
        .assert()
        .success();

    let config_path = home.join(".config/skillet/config.toml");
    let config = std::fs::read_to_string(&config_path).expect("read config");
    assert!(
        config.contains("test-registry"),
        "config should reference the local registry: {config}"
    );

    // Verify the registry is usable via config (no --registry flag needed)
    skillet()
        .args(["search", "rust"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("rust-dev"));
}

// ── Error recovery and edge cases (#146) ─────────────────────────

/// Malformed skill.toml in registry is skipped, valid skills still load
#[test]
fn scenario_malformed_skill_skipped() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let registry = tmp.path().join("registry");

    // Create a valid skill
    create_mini_skill(&registry, "good", "valid-skill", "A valid skill");

    // Create a malformed skill (bad TOML)
    let bad_skill = registry.join("bad/broken-skill");
    std::fs::create_dir_all(&bad_skill).expect("create bad skill dir");
    std::fs::write(bad_skill.join("skill.toml"), "[skill\nname = broken").expect("write bad toml");
    std::fs::write(bad_skill.join("SKILL.md"), "# Broken\n").expect("write SKILL.md");

    write_registry_config(&registry, "Mixed Registry");

    // Search should find the valid skill and skip the broken one
    let output = skillet()
        .args(["search", "*", "--registry"])
        .arg(&registry)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("valid-skill"),
        "valid skill should be found: {stdout}"
    );
    assert!(
        !stdout.contains("broken-skill"),
        "broken skill should be skipped: {stdout}"
    );
}

/// Empty registry returns no results without error
#[test]
fn scenario_empty_registry() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let registry = tmp.path().join("empty-registry");
    std::fs::create_dir_all(&registry).expect("create empty registry");
    write_registry_config(&registry, "Empty Registry");

    skillet()
        .args(["search", "*", "--registry"])
        .arg(&registry)
        .assert()
        .success()
        .stdout(predicate::str::contains("No skills found"));
}

/// Multi-registry where one has errors, other registry's skills still available
#[test]
fn scenario_mixed_registry_health() {
    let tmp = tempfile::tempdir().expect("create temp dir");

    // Good registry
    let good_reg = tmp.path().join("good-reg");
    create_mini_skill(&good_reg, "alice", "good-skill", "A working skill");
    write_registry_config(&good_reg, "Good Registry");

    // Bad registry: malformed skill only
    let bad_reg = tmp.path().join("bad-reg");
    let bad_skill = bad_reg.join("broken/bad-skill");
    std::fs::create_dir_all(&bad_skill).expect("create bad skill dir");
    std::fs::write(bad_skill.join("skill.toml"), "not valid toml {{{{").expect("write bad toml");
    std::fs::write(bad_skill.join("SKILL.md"), "# Bad\n").expect("write SKILL.md");
    write_registry_config(&bad_reg, "Bad Registry");

    // Search across both registries
    let output = skillet()
        .args(["search", "*", "--registry"])
        .arg(&good_reg)
        .args(["--registry"])
        .arg(&bad_reg)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("good-skill"),
        "good registry skills should still work: {stdout}"
    );
}

/// Corrupt cache is recovered gracefully
#[test]
fn scenario_corrupt_cache_recovery() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");

    // First search populates cache
    skillet()
        .args(["search", "rust", "--registry"])
        .arg(test_registry())
        .env("HOME", &home)
        .env("SKILLET_CACHE_DIR", &cache_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("rust-dev"));

    // Corrupt all cache files
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                std::fs::write(entry.path(), "not valid json {{{").expect("corrupt cache file");
            }
        }
    }

    // Second search should recover gracefully (rebuild from source)
    skillet()
        .args(["search", "rust", "--registry"])
        .arg(test_registry())
        .env("HOME", &home)
        .env("SKILLET_CACHE_DIR", &cache_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("rust-dev"));
}

/// Install from a registry that has a skill missing SKILL.md
#[test]
fn scenario_missing_skill_md_skipped() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let registry = tmp.path().join("registry");

    // Valid skill
    create_mini_skill(&registry, "owner", "good-skill", "Has everything");

    // Skill with only skill.toml, no SKILL.md
    let no_md = registry.join("owner/no-md-skill");
    std::fs::create_dir_all(&no_md).expect("create no-md skill dir");
    std::fs::write(
        no_md.join("skill.toml"),
        "[skill]\nname = \"no-md-skill\"\nowner = \"owner\"\nversion = \"1.0.0\"\ndescription = \"Missing SKILL.md\"\n",
    )
    .expect("write skill.toml");

    write_registry_config(&registry, "Test Registry");

    // Search should find the good skill, skip the one without SKILL.md
    let output = skillet()
        .args(["search", "*", "--registry"])
        .arg(&registry)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("good-skill"),
        "valid skill should be found: {stdout}"
    );
}

// ── npm repo compatibility ───────────────────────────────────────

/// search -> info -> install with rules/ files -> verify rules written to disk
#[test]
fn scenario_npm_repo_compatibility() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Step 1: Search finds all 3 skills
    let search_output = skillet()
        .args(["search", "*", "--registry"])
        .arg(test_npm_registry())
        .output()
        .expect("search");
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("redis-caching"),
        "search should find redis-caching: {search_stdout}"
    );
    assert!(
        search_stdout.contains("vector-search"),
        "search should find vector-search: {search_stdout}"
    );
    assert!(
        search_stdout.contains("session-management"),
        "search should find session-management: {search_stdout}"
    );

    // Step 2: Info shows frontmatter metadata
    skillet()
        .args(["info", "redis/redis-caching", "--registry"])
        .arg(test_npm_registry())
        .assert()
        .success()
        .stdout(predicate::str::contains("2.1.0").and(predicate::str::contains("caching")));

    // Step 3: Install skill with rules/ files
    skillet()
        .args(["install", "redis/redis-caching", "--registry"])
        .arg(test_npm_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed redis/redis-caching"));

    // Step 4: Verify files written to disk
    let skill_dir = tmp.path().join(".agents/skills/redis-caching");
    assert!(skill_dir.join("SKILL.md").exists(), "SKILL.md should exist");
    assert!(
        skill_dir.join("rules/cache-patterns.md").exists(),
        "rules/cache-patterns.md should exist"
    );
    assert!(
        skill_dir.join("rules/ttl-guidelines.md").exists(),
        "rules/ttl-guidelines.md should exist"
    );

    // Step 5: Verify content matches source
    let installed_md =
        std::fs::read_to_string(skill_dir.join("SKILL.md")).expect("read installed SKILL.md");
    let source_md =
        std::fs::read_to_string(test_npm_registry().join("skills/redis-caching/SKILL.md"))
            .expect("read source SKILL.md");
    assert_eq!(
        installed_md, source_md,
        "installed SKILL.md should match source"
    );

    // Step 6: Install skill with references/
    skillet()
        .args(["install", "redis/vector-search", "--registry"])
        .arg(test_npm_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    let vs_dir = tmp.path().join(".agents/skills/vector-search");
    assert!(
        vs_dir.join("references/embedding-guide.md").exists(),
        "references/embedding-guide.md should exist"
    );
}

// ── Official registry dogfood (#151) ──────────────────────────────

/// search -> info -> install from the in-repo official registry
#[test]
fn scenario_official_registry_dogfood() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Step 1: Search finds all official skills
    let search_output = skillet()
        .args(["search", "*", "--registry"])
        .arg(official_registry())
        .output()
        .expect("search");
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("skillet/user"),
        "search should find skillet/user: {search_stdout}"
    );
    assert!(
        search_stdout.contains("skillet/skill-author"),
        "search should find skillet/skill-author: {search_stdout}"
    );
    assert!(
        search_stdout.contains("skillet/contributor"),
        "search should find skillet/contributor: {search_stdout}"
    );

    // Step 2: Info on a specific skill
    skillet()
        .args(["info", "skillet/user", "--registry"])
        .arg(official_registry())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("skillet/user")
                .and(predicate::str::contains("version"))
                .and(predicate::str::contains("description")),
        );

    // Step 3: Install a skill
    skillet()
        .args(["install", "skillet/user", "--registry"])
        .arg(official_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed skillet/user"));

    // Step 4: Verify installed content
    let installed_md = std::fs::read_to_string(tmp.path().join(".agents/skills/user/SKILL.md"))
        .expect("read installed SKILL.md");
    let registry_md = std::fs::read_to_string(official_registry().join("skillet/user/SKILL.md"))
        .expect("read registry SKILL.md");
    assert_eq!(
        installed_md, registry_md,
        "installed SKILL.md should match registry content"
    );
}
