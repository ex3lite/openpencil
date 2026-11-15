//! Guards against untranslated Russian UI copy while allowing brands and protocol literals.

#[test]
fn russian_copy_does_not_fall_back_to_english_or_chinese() {
    let allowed = [
        "topbar.mcp",
        "code.reactTailwind",
        "code.htmlCss",
        "agents.claudeCode",
        "agents.codexCli",
        "agents.opencode",
        "agents.copilot",
        "agents.stdio",
        "agents.http",
        "agents.stdioHttp",
        "settings.mcp",
        "builtin.baseUrlPlaceholder",
        "acp.commandPlaceholder",
        "acp.argsPlaceholder",
        "acp.url",
        "acp.urlPlaceholder",
        "landing.open",
        "landing.pencil",
        "settings.tab.mcp",
        "settings.images.clientIdPlaceholder",
        "settings.images.clientSecretPlaceholder",
        "git.title",
        "git.picker.lastCommit",
        "git.autosave.messagePrefix",
        "git.branch.createPlaceholder",
        "git.wizard.clone.urlPlaceholder",
        "git.remote.urlPlaceholder",
        "git.ssh.hostPlaceholder",
        "git.ssh.commentPlaceholder",
        "git.ssh.importPathPlaceholder",
        "git.auth.modeSsh",
        "git.panel.title",
        "git.panel.branch",
        "git.panel.remotePlaceholder",
        "home.connect.cliNote",
        "collab.a11y.participant",
        "home.tools.figma",
    ];
    let english = include_str!("en.rs").to_string()
        + include_str!("en_git.rs")
        + include_str!("en_panel.rs")
        + include_str!("en_collab.rs");
    for line in english.lines() {
        let Some(rest) = line.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some((key, _)) = rest.split_once('"') else {
            continue;
        };
        let Some(en) = super::en::lookup(key) else {
            continue;
        };
        let ru = super::ru::lookup(key).expect("Russian catalog contains every English key");
        assert!(
            !ru.chars()
                .any(|c| matches!(c, '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')),
            "Chinese copy in Russian catalog: {key}: {ru}"
        );
        if en.bytes().any(|c| c.is_ascii_alphabetic()) {
            assert!(
                ru != en || allowed.contains(&key),
                "Untranslated Russian copy: {key}: {ru}"
            );
        }
    }
}
