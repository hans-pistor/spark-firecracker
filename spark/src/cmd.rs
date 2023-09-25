use std::{ffi::OsStr, process::Command};

#[derive(Debug)]
pub enum CommandNamespace {
    Named(String),
    Global
}

impl std::fmt::Display for CommandNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Named(namespace) => write!(f, "{}", namespace),
            Self::Global => write!(f, "global")
        }
    }
}

pub fn run<I, S>(
    network_namespace: &CommandNamespace,
    program: &'static str,
    args: I,
) -> Result<std::process::Output, std::io::Error> 
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr> {
        match network_namespace {
            CommandNamespace::Named(namespace) => run_in_namespace(&namespace, program, args),
            CommandNamespace::Global => run_inner(program, args)
        }
    }

fn run_in_namespace<I, S>(
    namespace: &str,
    program: &'static str,
    args: I,
) -> Result<std::process::Output, std::io::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut new_args = format!("netns exec {namespace} {program}");
    args.into_iter()
        .for_each(|s| new_args.push_str(&format!(" {}", &s.as_ref().to_string_lossy())));

    run_inner("ip", new_args.split(' '))
}

fn run_inner<I, S>(program: &'static str, args: I) -> Result<std::process::Output, std::io::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program).args(args).output();
    println!("{:?}", output);

    output
}