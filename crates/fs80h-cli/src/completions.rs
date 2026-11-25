use crate::cli::{Cli, CliError, CompletionShell, OutputFormat};
use clap::CommandFactory;
use clap_complete::{Shell, generate};
use std::fs;
use std::path::{Path, PathBuf};

fn error(context: &str, err: impl std::fmt::Display) -> CliError {
    CliError::Operational(format!("completion installation: {context}: {err}"))
}

fn home() -> Result<PathBuf, CliError> {
    let value = std::env::var_os("HOME").ok_or_else(|| {
        CliError::Operational("completion installation: HOME is unavailable".into())
    })?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(CliError::Operational(
            "completion installation: HOME must be absolute".into(),
        ));
    }
    Ok(path)
}

fn write_script(path: &Path, shell: Shell) -> Result<(), CliError> {
    let parent = path.parent().expect("completion path has parent");
    fs::create_dir_all(parent).map_err(|e| error("create completion directory", e))?;
    let mut bytes = Vec::new();
    generate(shell, &mut Cli::command(), "fs80hctl", &mut bytes);
    fs::write(path, bytes).map_err(|e| error("write completion script", e))
}

fn append_once(path: &Path, marker: &str, block: &str) -> Result<(), CliError> {
    let old = if path.exists() {
        fs::read_to_string(path).map_err(|e| error("read shell profile", e))?
    } else {
        String::new()
    };
    if old.contains(marker) {
        return Ok(());
    }
    let separator = if old.is_empty() || old.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::write(path, format!("{old}{separator}{block}"))
        .map_err(|e| error("update shell profile", e))
}

fn install_zsh(home: &Path) -> Result<PathBuf, CliError> {
    let script = home.join(".zfunc/_fs80hctl");
    write_script(&script, Shell::Zsh)?;
    let profile = home.join(".zshrc");
    let old = if profile.exists() {
        fs::read_to_string(&profile).map_err(|e| error("read Zsh profile", e))?
    } else {
        String::new()
    };
    let marker = "# fs80hctl completion";
    if !old.contains(marker) {
        let setup = "# fs80hctl completion\nfpath=(\"$HOME/.zfunc\" $fpath)\n";
        let updated = if let Some(index) = old.lines().position(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with('#') && trimmed.split_whitespace().any(|word| word == "compinit")
        }) {
            let offset = old
                .lines()
                .take(index)
                .map(|line| line.len() + 1)
                .sum::<usize>();
            format!("{}{setup}{}", &old[..offset], &old[offset..])
        } else {
            let separator = if old.is_empty() || old.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            format!("{old}{separator}{setup}autoload -Uz compinit\ncompinit\n")
        };
        fs::write(&profile, updated).map_err(|e| error("update Zsh profile", e))?;
    }
    Ok(script)
}

pub fn install(shell: CompletionShell, format: OutputFormat) -> Result<(), CliError> {
    let home = home()?;
    let path = match shell {
        CompletionShell::Bash => {
            let script = home.join(".local/share/fs80hctl/completions/fs80hctl.bash");
            write_script(&script, Shell::Bash)?;
            append_once(
                &home.join(".bashrc"),
                "# fs80hctl completion",
                "# fs80hctl completion\nsource \"$HOME/.local/share/fs80hctl/completions/fs80hctl.bash\"\n",
            )?;
            script
        }
        CompletionShell::Zsh => install_zsh(&home)?,
        CompletionShell::Fish => {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".config"));
            let script = config.join("fish/completions/fs80hctl.fish");
            write_script(&script, Shell::Fish)?;
            script
        }
        CompletionShell::Powershell => {
            let script = home.join(".config/fs80hctl/completions/fs80hctl.ps1");
            write_script(&script, Shell::PowerShell)?;
            let profile = if cfg!(windows) {
                home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1")
            } else {
                home.join(".config/powershell/Microsoft.PowerShell_profile.ps1")
            };
            fs::create_dir_all(profile.parent().expect("profile has parent"))
                .map_err(|e| error("create PowerShell profile directory", e))?;
            append_once(
                &profile,
                "# fs80hctl completion",
                ". \"$HOME/.config/fs80hctl/completions/fs80hctl.ps1\" # fs80hctl completion\n",
            )?;
            script
        }
    };
    if format == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({"status":"installed", "shell": format!("{shell:?}").to_lowercase(), "path":path})
        );
    } else {
        println!(
            "Installed fs80hctl completion at {}. Open a new shell to use it.",
            path.display()
        );
    }
    Ok(())
}
