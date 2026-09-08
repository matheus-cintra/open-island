use std::process::{Command, Output};

pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String>;
}

pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        Command::new(program)
            .args(args)
            .output()
            .map_err(|error| format!("failed to run {program}: {error}"))
    }
}

#[cfg(test)]
pub struct FakeRunner {
    responses: std::sync::Mutex<Vec<(String, Result<Output, String>)>>,
    calls: std::sync::Mutex<Vec<(String, Vec<String>)>>,
}

#[cfg(test)]
impl FakeRunner {
    pub fn new() -> Self {
        Self {
            responses: std::sync::Mutex::new(Vec::new()),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn push_ok(&self, program: &str, stdout: &str) {
        self.push_status(program, 0, stdout, "");
    }

    pub fn push_status(&self, program: &str, code: i32, stdout: &str, stderr: &str) {
        use std::os::unix::process::ExitStatusExt;
        self.responses.lock().unwrap().push((
            program.to_owned(),
            Ok(Output {
                status: std::process::ExitStatus::from_raw(code),
                stdout: stdout.as_bytes().to_vec(),
                stderr: stderr.as_bytes().to_vec(),
            }),
        ));
    }

    pub fn push_err(&self, program: &str, message: &str) {
        self.responses
            .lock()
            .unwrap()
            .push((program.to_owned(), Err(message.to_owned())));
    }

    pub fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl Default for FakeRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        self.calls.lock().unwrap().push((
            program.to_owned(),
            args.iter().map(|arg| (*arg).to_owned()).collect(),
        ));
        let mut responses = self.responses.lock().unwrap();
        let position = responses.iter().position(|(name, _)| name == program);
        position
            .map(|index| responses.remove(index).1)
            .unwrap_or_else(|| Err(format!("no scripted response for {program}")))
    }
}
