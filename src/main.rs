use chrono::prelude::*;
use itertools::Itertools;
use std::{
    collections::HashMap,
    env, fmt,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, Read, Write},
    path::PathBuf,
};

/// Levenshtein distance
fn lev(a: &str, b: &str) -> usize {
    if b.is_empty() {
        a.len()
    } else if a.is_empty() {
        b.len()
    } else if a.chars().next() == b.chars().next() {
        lev(&a[1..], &b[1..])
    } else {
        1 + lev(&a[1..], b)
            .min(lev(a, &b[1..]))
            .min(lev(&a[1..], &b[1..]))
    }
}

fn close_match(a: &str, b: &str) -> bool {
    lev(a, b) <= usize::min(usize::min(a.len(), b.len()) / 2, 3)
}

/// Prompt for a yes/no answer
fn yes_or_no(prompt: &str) -> bool {
    loop {
        print!("{} [y/n] :", prompt);
        io::stdout().flush().unwrap();
        let stdin = io::stdin();
        let answer = stdin.lock().lines().next().unwrap().unwrap();
        if answer.starts_with(|c: char| c.to_ascii_lowercase() == 'y') {
            println!();
            return true;
        } else if answer.starts_with(|c: char| c.to_ascii_lowercase() == 'n') {
            println!();
            return false;
        } else {
            continue;
        }
    }
}

type ParseError = String;

enum StreakState {
    Done,
    Pending,
    Expired,
    New,
}

impl StreakState {
    fn serialize(&self) -> &'static str {
        match self {
            StreakState::Done => "Done",
            StreakState::Pending => "Pending",
            StreakState::Expired => "Expired",
            StreakState::New => "New",
        }
    }

    fn deserialize(string: &str) -> Result<Self, ParseError> {
        match string {
            "Done" => Ok(StreakState::Done),
            "Pending" => Ok(StreakState::Pending),
            "Expired" => Ok(StreakState::Expired),
            "New" => Ok(StreakState::New),
            _ => Err(format!("unknown streak state: \"{}\"", string)),
        }
    }
}

struct Streak {
    current_count: u32,
    max_count: u32,
    last_hit: DateTime<Local>,
    state: StreakState,
}

impl Streak {
    fn new() -> Self {
        Self {
            current_count: 0,
            max_count: 0,
            last_hit: Local::now(),
            state: StreakState::New,
        }
    }

    fn update_count<F: FnOnce(u32) -> u32>(&mut self, action: F) {
        self.current_count = action(self.current_count);
        self.max_count = self.max_count.max(self.current_count);
    }

    /// Returns the new streak count if it updated
    fn hit(&mut self, disambiguator: Option<String>) -> Option<u32> {
        match self.state {
            StreakState::Done => {
                if let Some(s) = disambiguator {
                    eprint!("{}", s);
                }
                eprintln!("streak already completed today");
                None
            }
            StreakState::Expired | StreakState::New => {
                self.state = StreakState::Done;
                self.last_hit = Local::now();
                self.update_count(|_old_count| 1);
                Some(self.current_count)
            }
            StreakState::Pending => {
                self.state = StreakState::Done;
                self.last_hit = Local::now();
                self.update_count(|old_count| old_count + 1);
                Some(self.current_count)
            }
        }
    }

    fn serialize(&self) -> String {
        format!(
            "{},{},{},{}",
            self.current_count,
            self.max_count,
            self.last_hit,
            self.state.serialize()
        )
    }

    fn deserialize(values: &[&str]) -> Result<Self, ParseError> {
        match values.len() {
            4 => Ok(Self {
                current_count: values[0].parse::<u32>().map_err(|err| {
                    format!("expected unsigned integer for current_count: {}", err)
                })?,
                max_count: values[1]
                    .parse::<u32>()
                    .map_err(|err| format!("expected unsigned integer for max_count: {}", err))?,
                last_hit: values[2]
                    .parse::<DateTime<Local>>()
                    .map_err(|err| format!("expected local datetime for last_hit: {}", err))?,
                state: StreakState::deserialize(values[3])?,
            }),
            _ => Err(format!(
                "expected 4 comma-separated values for a streak description, got {}: \"{}\"",
                values.len(),
                values.join(",")
            )),
        }
    }
}

struct State {
    streaks: HashMap<String, Streak>,
}

impl State {
    fn not_found(&mut self, name: &str) {
        eprint!("streak \"{}\" not found", name);
        if let Some(alt_name) = self.streaks.keys().find(|n| close_match(n, name)) {
            eprintln!(", maybe you meant \"{}\"?", alt_name);
        } else {
            eprintln!();
        }
    }

    fn update(&mut self) {
        let now = Local::now();
        for (_, streak) in self.streaks.iter_mut() {
            let days_between = now.num_days_from_ce() - streak.last_hit.num_days_from_ce();
            match days_between {
                0 => (),
                1 => {
                    streak.state = StreakState::Pending;
                }
                n if n > 1 => {
                    streak.state = StreakState::Expired;
                    streak.update_count(|_old_count| 0);
                }
                _ => {
                    eprintln!("corrupted time state");
                    streak.state = StreakState::Pending;
                    streak.update_count(|_old_count| 0);
                }
            };
        }
    }

    fn add_streaks(&mut self, names: &[String]) {
        // TODO: guard this more
        for name in names.iter() {
            if self.streaks.insert(name.clone(), Streak::new()).is_some() {
                eprintln!("warning: reset old version of streak \"{}\"", name);
            }
        }
    }

    fn remove_streaks(&mut self, names: &[String]) {
        for name in names.iter() {
            if self.streaks.remove(name).is_none() {
                self.not_found(name);
            }
        }
    }

    fn rename_streak(&mut self, name: &str, new_name: &str) {
        if let Some(streak) = self.streaks.remove(name) {
            self.streaks.insert(new_name.to_owned(), streak);
        } else {
            self.not_found(name);
        }
    }

    /// Returns the new streak count if it updated
    fn hit_streak(&mut self, name: &str, one_of_many: bool) -> Option<u32> {
        let disambiguator = one_of_many.then(|| format!("\"{}\": ", name));
        if let Some(streak) = self.streaks.get_mut(name) {
            return streak.hit(disambiguator);
        }
        if let Some(alt_name) = self.streaks.keys().find(|n| close_match(n, name)) {
            eprintln!("streak with a similar name exists: \"{}\"", alt_name);
            if yes_or_no("hit this streak?") {
                let alt_name = alt_name.clone();
                return self.streaks.get_mut(&alt_name).unwrap().hit(disambiguator);
            }
        }
        eprintln!("creating new streak \"{}\"", name);
        self.streaks
            .entry(name.to_owned())
            .or_insert_with(Streak::new)
            .hit(disambiguator)
    }

    fn serialize(&self) -> String {
        let mut lines = Vec::new();
        for (name, streak) in self.streaks.iter().sorted_by_key(|pair| pair.0) {
            lines.push(format!("{},{}", name, streak.serialize()));
        }
        lines.join("\n")
    }

    fn deserialize(string: &str) -> Result<Self, ParseError> {
        let mut streaks = HashMap::new();
        for (line_number, line) in string.lines().enumerate() {
            let values: Vec<_> = line.split(',').collect();
            if values.len() < 2 {
                return Err(format!(
                    "expected name and state for streak on line {}: \"{}\"",
                    line_number + 1,
                    line
                ));
            }
            streaks.insert(
                values[0].to_owned(),
                Streak::deserialize(&values[1..]).map_err(|err| {
                    format!(
                        "failed to parse streak on line {}: {}",
                        line_number + 1,
                        err
                    )
                })?,
            );
        }
        Ok(Self { streaks })
    }
}

fn write_table(f: &mut fmt::Formatter, table: Vec<[String; 4]>) -> fmt::Result {
    let max_widths: Vec<_> = (0..4)
        .map(|i| table.iter().map(|arr| arr[i].len()).max().unwrap())
        .collect();
    for row in table {
        writeln!(
            f,
            "{:<width0$} {:>width1$} {:>width2$} {:>width3$}",
            row[0],
            row[1],
            row[2],
            row[3],
            width0 = max_widths[0],
            width1 = max_widths[1],
            width2 = max_widths[2],
            width3 = max_widths[3]
        )?;
    }
    Ok(())
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !self.streaks.is_empty() {
            let table: Vec<_> = self
                .streaks
                .iter()
                .sorted_by_key(|pair| pair.0)
                .map(|pair| {
                    let (name, streak) = pair;
                    [
                        format!("- {}:", name),
                        format!("{}", streak.current_count),
                        format!("(max {})", streak.max_count),
                        streak.state.serialize().to_owned(),
                    ]
                })
                .collect();
            write_table(f, table)?;
        }
        Ok(())
    }
}

fn print_usage(path: &str) {
    println!("usage: {} <command> [args...]", path);
    println!();
    println!("supported commands:");
    println!();
    println!("    display - Output a list of streaks with information about their state.");
    println!("    update - Check the date and update pending/expired state of streaks.");
    println!("    hit <streak name> - Hit a streak with the given name.");
    println!("    add <streak name> - Start tracking a new streak with the given name.");
    println!("    remove <streak name> - Stop tracking the streak with the given name.");
    println!("    rename <streak name> <new name> - Change the name of an existing streak.");
}

fn ensure_state_path() -> PathBuf {
    let mut path = dirs::data_dir().expect("couldn't locate directory to store data");
    path.push("streaks");
    if let Err(err) = fs::create_dir_all(&path) {
        panic!("couldn't create directory for storing state data: {}", err);
    }
    path.push("state.txt");
    path
}

fn read_string(mut file: File) -> io::Result<String> {
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn read_state() -> State {
    let path = ensure_state_path();
    match OpenOptions::new()
        .read(true)
        // we need write(true) for create(true) to work
        .write(true)
        .truncate(false)
        .create(true)
        .open(&path)
    {
        Ok(file) => match read_string(file) {
            Ok(string) => match State::deserialize(&string) {
                Ok(state) => state,
                Err(err) => panic!("couldn't parse state file: {}", err),
            },
            Err(err) => panic!("couldn't read state file: {}", err),
        },
        Err(err) => panic!("couldn't open state file: {}", err),
    }
}

fn write_state(state: State) {
    let path = ensure_state_path();
    match OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&path)
    {
        Ok(mut file) => {
            if let Err(err) = write!(file, "{}", state.serialize()) {
                eprintln!("couldn't write state file: {}", err);
            }
        }
        Err(err) => eprintln!("couldn't open state file: {}", err),
    }
}

fn modify_state<F: FnOnce(&mut State)>(action: F) {
    let mut state = read_state();
    action(&mut state);
    write_state(state);
}

fn display_state() {
    print!("{}", read_state());
}

fn run_command(path: &str, command: &str, args: &[String]) {
    match command {
        "update" => {
            modify_state(|state| state.update());
            println!("updated streak states");
        }
        "hit" => {
            if args.is_empty() {
                eprintln!("expected an argument");
            } else {
                let mut count = None;
                for arg in args.iter() {
                    modify_state(|state| {
                        count = state.hit_streak(arg, true);
                    });
                    if let Some(count) = count {
                        println!("hit streak \"{}\": now at {}", arg, count);
                    }
                }
            }
        }
        "add" => {
            if args.is_empty() {
                eprintln!("expected an argument");
            } else {
                modify_state(|state| state.add_streaks(&args));
                for arg in args.iter() {
                    println!("added streak \"{}\"", arg);
                }
            }
        }
        "remove" => {
            if args.is_empty() {
                eprintln!("expected an argument");
            } else {
                modify_state(|state| state.remove_streaks(&args));
                for arg in args.iter() {
                    println!("removed streak \"{}\"", arg);
                }
            }
        }
        "rename" => {
            if args.len() != 2 {
                eprintln!("expected 2 arguments");
            } else {
                modify_state(|state| state.rename_streak(&args[0], &args[1]));
                println!("renamed streak \"{}\" to \"{}\"", &args[0], &args[1]);
            }
        }
        "display" => display_state(),
        _ => {
            eprintln!("unknown command {}", command);
            print_usage(path);
        }
    }
}

fn main() {
    let args: Vec<_> = env::args().collect();
    if args.len() < 2 {
        print_usage(&args[0]);
    } else {
        run_command(&args[0], &args[1], &args[2..]);
    }
}
