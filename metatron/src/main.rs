use std::io::Write;

const MARKDOWN_PROMPT: &str = r#"
you will receive an XML input of structure:
- <files>
  - list of <file><name>Filename</name><contents>File Contents</contents></file>

using this, you are to generate a README.md file with the following sections:
- Title 
  - Titled with the name of the project, containing a short, punchy description of what the project is and its purpose
- Examples
  - This should cover only the absolute bare minimum that would be necessary to get a user up and running with the project
- Installation
  - How to install the project to be used in its appropriate context (through package manager, clone + compile, etc.)
  - This needs to be extremely thorough and precise

note: generate _only_ the markdown for the readme--nothing else!
"#;

const TMP_FILE: &str = "/tmp/alfred.tmp";

// copied code from Bernard
fn prompt_tllm(system_prompt: &str, input: &str) -> Result<String, std::io::Error> {
    {
        let mut file = std::fs::File::create(TMP_FILE)?;
        file.write_all(input.as_bytes())?;
    }

    let output = match std::process::Command::new("tllm")
        .arg("-s")
        .arg(system_prompt)
        .arg("-n")
        .arg("-a")
        .arg("gemini")
        .arg("-i")
        .arg(TMP_FILE)
        .output()
    {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(e) => {
            println!("Error reading tllm output: {}", e);
            return Err(e);
        }
    };

    std::fs::remove_file(TMP_FILE)?;

    Ok(output)
}

macro_rules! prompt {
    ($p:expr, $input:expr) => {
        match prompt_tllm($p, $input) {
            Ok(response) => response,
            Err(e) => {
                println!("error prompting TLLM: {}", e);
                return Err(e);
            }
        }
    };
}

fn main() -> Result<(), std::io::Error> {
    let args = std::env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        println!("Usage: {} <directory|filename>", args[0]);
        std::process::exit(1);
    }

    let mut target = args[1].clone();
    if std::path::Path::new(&target).is_dir() {
        if target.ends_with("/") {
            target.push_str("**/*");
        } else {
            target.push_str("/**/*");
        }
    }

    let mut gitignore_pattern = target.clone();
    if target.ends_with("/") {
        gitignore_pattern.push_str("**/.gitignore");
    } else {
        gitignore_pattern.push_str("/**/.gitignore");
    }

    let globbed = glob::glob(&target).expect("Failed to create glob pattern");

    let mut blacklist = std::collections::HashSet::new();
    for file in globbed.into_iter() {
        let file = file.unwrap();
        if file.is_file() && file.ends_with(".gitignore") {
            let path = std::path::Path::new(&file).parent().unwrap();
            let contents = std::fs::read_to_string(&file).expect("Failed to read gitignore");
            println!("{}", contents);
            let entries = contents
                .lines()
                .filter(|line| !line.starts_with('#') && !line.is_empty())
                .collect::<Vec<&str>>();

            for entry in entries {
                let entry = if entry.starts_with("/") {
                    entry.trim_start_matches('/')
                } else {
                    entry
                };

                let mut full_path = path.join(entry);
                blacklist.insert(full_path.to_string_lossy().into_owned());

                full_path = full_path.join("**/*");
                blacklist.insert(full_path.to_string_lossy().into_owned());
            }
        } else if file.to_string_lossy().contains(".git") {
            let git_path = file.to_string_lossy();
            let git_path = git_path.split(".git").next().unwrap();
            if git_path.ends_with("/") {
                blacklist.insert(format!("{}.git/**/*", git_path));
            } else {
                blacklist.insert(format!("{}/.git/**/*", git_path));
            }
        }
    }

    let blacklist: Vec<String> = blacklist.into_iter().collect::<Vec<String>>();
    let globbed = glob::glob(&target).expect("Failed to create glob pattern");
    let mut xml = "<files>".to_string();
    for source in globbed {
        let source = source.unwrap();
        if source.is_dir()
            || blacklist.iter().any(|g| {
                glob::Pattern::new(g)
                    .unwrap()
                    .matches(&source.to_string_lossy())
            })
        {
            continue;
        }

        let contents = match std::fs::read_to_string(&source) {
            Ok(c) => c,
            Err(e) => {
                println!(
                    "Excluding file {}, error reading: {}",
                    source.to_str().unwrap(),
                    e
                );
                continue;
            }
        };

        xml.push_str(&format!(
            "<file><name>{}</name><contents>{}</contents></file>",
            source.to_str().unwrap(),
            contents
        ));
    }

    xml.push_str("</file>");

    let response = prompt!(MARKDOWN_PROMPT, &xml);

    println!("{}", response);

    Ok(())
}
