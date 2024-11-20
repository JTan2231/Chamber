#[cfg(debug_assertions)]
const DEBUG: bool = true;
#[cfg(not(debug_assertions))]
const DEBUG: bool = false;

use std::io::Write;

mod logger;

use crate::logger::Logger;

fn get_directory(target: String) -> String {
    let globbed = glob::glob(&target).expect("Failed to create glob pattern");
    let globbed = globbed
        .into_iter()
        .map(|p| p.unwrap())
        .collect::<Vec<std::path::PathBuf>>();

    let mut blacklist = std::collections::HashSet::new();
    for file in globbed.iter() {
        if file.is_file() && file.ends_with(".gitignore") {
            let path = std::path::Path::new(&file).parent().unwrap();
            let contents = std::fs::read_to_string(&file).expect("Failed to read gitignore");
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
    let mut file_list = Vec::new();
    let mut shortest = String::new();

    for source in globbed.iter() {
        if globbed.len() > 4096 && source.is_file() {
            continue;
        }

        let source = source.to_str().unwrap();
        if blacklist
            .iter()
            .any(|g| glob::Pattern::new(g).unwrap().matches(&source.to_string()))
        {
            continue;
        }

        if source.len() < shortest.len() {
            shortest = source.to_string();
        }

        file_list.push(format!("{}\n", source));
    }

    let schars = shortest.chars().collect::<Vec<char>>();
    let mut longest_prefix = schars.len();
    for (i, c) in schars.iter().enumerate() {
        for file in file_list.iter() {
            if file.chars().nth(i as usize).unwrap() != *c {
                longest_prefix = i;
                break;
            }
        }
    }

    file_list
        .iter()
        .map(|f| f.chars().skip(longest_prefix as usize).collect())
        .collect::<Vec<String>>()
        .join("\n")
}

// copied code from Bernard
fn prompt_tllm(system_prompt: &str, input: &str) -> Result<String, std::io::Error> {
    match std::process::Command::new("tllm")
        .arg("-s")
        .arg(system_prompt)
        .arg("-n")
        .arg("-i")
        .arg(input)
        .output()
    {
        Ok(output) => Ok(String::from_utf8_lossy(&output.stdout).to_string()),
        Err(e) => {
            error!("Error reading tllm output: {}", e);
            return Err(e);
        }
    }
}

macro_rules! prompt {
    ($p:expr, $input:expr) => {
        match prompt_tllm($p, $input) {
            Ok(response) => response,
            Err(e) => {
                error!("error prompting TLLM: {}", e);
                return Err(e);
            }
        }
    };
}

const SYSTEM_PROMPT: &str = r#"
you will be given two items:
- <request> -- a request from the user for you to interact with the file system
- <directory> -- a recursive list of files in the current working directory

write a bash script to perform the user's request. be thorough in your code comments in explaining what's happening.
_be cautious_
respond only in bash code.
"#;

fn main() -> Result<(), std::io::Error> {
    let now = match DEBUG {
        true => "debug".to_string(),
        false => chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string(),
    };

    let home = match std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| {
            std::env::var("HOMEDRIVE").and_then(|homedrive| {
                std::env::var("HOMEPATH").map(|homepath| format!("{}{}", homedrive, homepath))
            })
        }) {
        Ok(dir) => std::path::PathBuf::from(dir),
        Err(_) => panic!("Failed to get home directory"),
    };

    let logs = home.join(".local/alfred/logs");
    if !logs.exists() {
        match std::fs::create_dir_all(&logs) {
            Ok(_) => (),
            Err(e) => panic!("Failed to create directory: {:?}, {}", logs, e),
        };
    }

    Logger::init(format!("{}/{}.log", logs.to_str().unwrap(), now.clone()));

    let args = std::env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        println!("Usage: {} <request>", args[0]);
        std::process::exit(1);
    }

    let request = args[1].clone();
    info!("running alfred with request: {}", request);

    let mut target = match std::env::current_dir() {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(e) => {
            error!("Error getting current directory: {}", e);
            return Err(e);
        }
    };

    if std::path::Path::new(&target).is_dir() {
        if target.ends_with("/") {
            target.push_str("**/*");
        } else {
            target.push_str("/**/*");
        }
    }

    let mut feedback = Vec::new();
    let mut input = String::new();
    while input.trim() != "q" {
        input.clear();

        let mut prompt = format!(
            "<request>{}</request>\n<directory>{}</directory>",
            request,
            get_directory(target.clone())
        );

        for fb in feedback.iter() {
            prompt.push_str(&format!("\n<feedback>{}</feedback>", fb));
        }

        let script = prompt!(SYSTEM_PROMPT, &prompt);
        print!("{}", script);
        print!("Is this good to execute? (y/n/q): ");
        std::io::stdout().flush().unwrap();

        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read user input");

        println!("");

        if input.trim() == "n" {
            let mut fb = String::new();
            print!("Provide some feedback: ");
            std::io::stdout().flush().unwrap();
            std::io::stdin()
                .read_line(&mut fb)
                .expect("Failed to read user input");

            println!("");

            feedback.push(fb);
        } else if input.trim() == "y" {
            let filename = "alfred_temp.sh";
            let mut file = std::fs::File::create(filename)?;
            file.write_all(script.as_bytes())?;

            std::process::Command::new("chmod")
                .arg("+x")
                .arg(filename)
                .status()?;

            let output = std::process::Command::new("bash").arg(filename).output()?;

            std::io::stdout().write_all(&output.stdout)?;
            std::io::stderr().write_all(&output.stderr)?;

            std::fs::remove_file(filename)?;
        }
    }

    Ok(())
}
