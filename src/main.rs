use chrono::prelude::*;
use std::{collections::HashMap, env, fmt};

enum StreakState {
    Done,
    Pending,
    Expired,
}

impl StreakState {
    fn serialize(&self) -> &'static str {
        match self {
            StreakState::Done => "Done",
            StreakState::Pending => "Pending",
            StreakState::Expired => "Expired",
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
            state: StreakState::Done,
        }
    }

    fn hit(&mut self) {
        match self.state {
            StreakState::Done => eprintln!("streak already completed today"),
            StreakState::Expired => {
                self.state = StreakState::Done;
                self.current_count = 1;
            }
            StreakState::Pending => {
                self.state = StreakState::Done;
                self.current_count += 1;
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
}

struct State {
    streaks: HashMap<String, Streak>,
}

impl State {
    fn update(&mut self) {
        let now = Local::now();
        for (_, streak) in self.streaks.iter_mut() {
            let days_between = now.num_days_from_ce() - streak.last_hit.num_days_from_ce();
            match days_between {
                0 => {
                    streak.state = StreakState::Done;
                }
                1 => {
                    streak.state = StreakState::Pending;
                }
                n if n > 1 => {
                    streak.state = StreakState::Expired;
                    streak.current_count = 0;
                }
                _ => {
                    eprintln!("corrupted time state");
                    streak.state = StreakState::Pending;
                    streak.current_count = 0;
                }
            };
        }
    }

    fn add_streak(&mut self, name: &str) {
        // TODO: guard this more
        if self
            .streaks
            .insert(name.to_owned(), Streak::new())
            .is_some()
        {
            eprintln!("warning: reset old version of that streak");
        }
    }

    fn remove_streak(&mut self, name: &str) {
        if self.streaks.remove(name).is_none() {
            eprintln!("no streak called \"{}\" exists", name);
        }
    }

    fn hit_streak(&mut self, name: &str) {
        match self.streaks.get_mut(name) {
            Some(streak) => streak.hit(),
            None => {
                let mut streak = Streak::new();
                streak.hit();
                self.streaks.insert(name.to_owned(), streak);
            }
        }
    }

    fn serialize(&self) -> String {
        let mut lines = Vec::new();
        for (name, streak) in self.streaks.iter() {
            lines.push(format!("{},{}", name, streak.serialize()));
        }
        lines.join("\n")
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (name, streak) in self.streaks.iter() {
            writeln!(
                f,
                "{}: {} (max {}) {}",
                name,
                streak.current_count,
                streak.max_count,
                streak.state.serialize()
            )?;
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
}

fn read_state() -> State {
    // TODO
}

fn write_state(state: State) {
    // TODO
}

fn modify_state<F: FnOnce(&mut State)>(action: F) {
    let mut state = read_state();
    action(&mut state);
    write_state(state);
}

fn display_state() {
    println!("{}", read_state());
}

fn run_command(path: &str, command: &str, args: &[String]) {
    match command {
        "update" => modify_state(|state| state.update()),
        "hit" => {
            if args.len() != 1 {
                eprintln!("expected 1 argument");
            } else {
                modify_state(|state| state.hit_streak(&args[0]));
            }
        }
        "add" => {
            if args.len() != 1 {
                eprintln!("expected 1 argument");
            } else {
                modify_state(|state| state.add_streak(&args[0]));
            }
        }
        "remove" => {
            if args.len() != 1 {
                eprintln!("expected 1 argument");
            } else {
                modify_state(|state| state.remove_streak(&args[0]));
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
