#![crate_name = "uu_wc"]


//// This is a re-write of https://github.com/uutils/coreutils/tree/master/src/wc
////     using https://github.com/kbknapp/clap-rs as an example of common usage. 
//// Most of the structure of the program is the same for the sake of 
////     simplicity and readability.
//// Comments with '////' are mine
//// License (for both projects): MIT


////ORIGINAL HEADER:
/*
 * This file is part of the uutils coreutils package.
 *
 * (c) Boden Garman <bpgarman@gmail.com>
 *
 * For the full copyright and license information, please view the LICENSE
 * file that was distributed with this source code.
 */

extern crate libc;
extern crate clap;  //// use clap instead of getopts

#[macro_use]
extern crate uucore;

use std::ascii::AsciiExt;
use std::fs::File;
use std::io::{stdin, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::result::Result as StdResult;
use std::str::from_utf8;
use clap::{App, Arg, ArgMatches};

struct Settings {
    show_bytes: bool,
    show_chars: bool,
    show_lines: bool,
    show_words: bool,
    show_max_line_length: bool,
}

impl Settings {
    fn new(matches: &ArgMatches) -> Settings {
        let settings = Settings {
            ////clap syntax for checking whether flags are present
            show_bytes: matches.is_present("bytes"),
            show_chars: matches.is_present("chars"),
            show_lines: matches.is_present("lines"),
            show_words: matches.is_present("words"),
            show_max_line_length: matches.is_present("max-line-length"),
        };

        if settings.show_bytes
            || settings.show_chars
            || settings.show_lines
            || settings.show_words
            || settings.show_max_line_length {
            return settings;
        }

        ////default behavior in case no flags were specified
        Settings {
            show_bytes: true,
            show_chars: false,
            show_lines: true,
            show_words: true,
            show_max_line_length: false,
        }
    }
}

struct Result {
    title: String,
    bytes: usize,
    chars: usize,
    lines: usize,
    words: usize,
    max_line_length: usize,
}

static NAME: &'static str = "wc";
static VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn uumain(args: Vec<String>) -> i32 {

    ////define App and relevant possible arguments
    let matches : ArgMatches = App::new(NAME)
                        .version(VERSION)
                        .about("wc, courtesy of uutils, rewritten to demonstrate clap-rs usage")
                        .arg(Arg::with_name("bytes")
                            .short("c")
                            .help("print the byte counts"))
                        .arg(Arg::with_name("chars")
                            .short("m")
                            .help("print the character counts"))
                        .arg(Arg::with_name("lines")
                            .short("l")
                            .help("print the newline counts"))
                        .arg(Arg::with_name("max-line-length")
                            .short("L")
                            .help("print the length of the longest line"))
                        .arg(Arg::with_name("words")
                            .short("w")
                            .help("print the word counts"))
                         .arg(Arg::with_name("files")
                            ////can take multiple files (e.g. ./wc -l foo -m bar -c)
                            ////allows for star-expansion (e.g. ./wc *.rs)
                             .multiple(true)
                             .takes_value(true))
                        //.get_matches();
                        .get_matches_from(args);
    ////extract vector of file names
    let files = matches.values_of("files");
    ////default value is ["-"], indicating text will be typed/piped and closed with EOF
    let files = match files.is_some() {
        true  => files.unwrap().collect(),
        false => vec!["-".as_ref()],
    };

    ////generate settings struct from App::get_matches()
    ////not strictly necessary to use Settings struct (could be done only with clap),
    //// but this makes it easier to compare with getopts (which the program originally used),
    //// and Settings offers a tidy way to deal with the case of using default flags.
    let settings = Settings::new(&matches);
    match wc(files, &settings){
        Ok(()) => ( ),
        Err(e) => return e
    }

    0
}

const CR: u8 = '\r' as u8;
const LF: u8 = '\n' as u8;
const SPACE: u8 = ' ' as u8;
const TAB: u8 = '\t' as u8;
const SYN: u8 = 0x16 as u8;
const FF: u8 = 0x0C as u8;

#[inline(always)]
fn is_word_seperator(byte: u8) -> bool {
    byte == SPACE || byte == TAB || byte == CR || byte == SYN || byte == FF
}

fn wc(files: Vec<&str>, settings: &Settings) -> StdResult<(), i32> {
    let mut total_line_count: usize = 0;
    let mut total_word_count: usize = 0;
    let mut total_char_count: usize = 0;
    let mut total_byte_count: usize = 0;
    let mut total_longest_line_length: usize = 0;

    let mut results = vec!();
    let mut max_width: usize = 0;

    for path in &files {
        let mut reader = try!(open(&path[..]));

        let mut line_count: usize = 0;
        let mut word_count: usize = 0;
        let mut byte_count: usize = 0;
        let mut char_count: usize = 0;
        let mut longest_line_length: usize = 0;
        let mut raw_line = Vec::new();

        // reading from a TTY seems to raise a condition on, rather than return Some(0) like a file.
        // hence the option wrapped in a result here
        while match reader.read_until(LF, &mut raw_line) {
            Ok(n) if n > 0 => true,
            Err(ref e) if !raw_line.is_empty() => {
                show_warning!("Error while reading {}: {}", path, e);
                !raw_line.is_empty()
            },
            _ => false,
        } {
            // GNU 'wc' only counts lines that end in LF as lines
            if *raw_line.last().unwrap() == LF {
                line_count += 1;
            }

            byte_count += raw_line.len();

            // try and convert the bytes to UTF-8 first
            let current_char_count;
            match from_utf8(&raw_line[..]) {
                Ok(line) => {
                    word_count += line.split_whitespace().count();
                    current_char_count = line.chars().count();
                },
                Err(..) => {
                    word_count += raw_line.split(|&x| is_word_seperator(x)).count();
                    current_char_count = raw_line.iter().filter(|c|c.is_ascii()).count()
                }
            }
            char_count += current_char_count;

            if current_char_count > longest_line_length {
                // we subtract one here because `line.len()` includes the LF
                // matches GNU 'wc' behaviour
                longest_line_length = current_char_count - 1;
            }

            raw_line.truncate(0);
        }

        results.push(Result {
            title: path.to_string(),    ////clap passes Vec<&str> instead of Vec<String>
            bytes: byte_count,
            chars: char_count,
            lines: line_count,
            words: word_count,
            max_line_length: longest_line_length,
        });

        total_line_count += line_count;
        total_word_count += word_count;
        total_char_count += char_count;
        total_byte_count += byte_count;

        if longest_line_length > total_longest_line_length {
            total_longest_line_length = longest_line_length;
        }

        // used for formatting
        max_width = total_byte_count.to_string().len() + 1;
    }

    for result in &results {
        print_stats(settings, &result, max_width);
    }

    if files.len() > 1 {
        let result = Result {
            title: "total".to_owned(),
            bytes: total_byte_count,
            chars: total_char_count,
            lines: total_line_count,
            words: total_word_count,
            max_line_length: total_longest_line_length,
        };
        print_stats(settings, &result, max_width);
    }

    Ok(())
}

fn print_stats(settings: &Settings, result: &Result, max_width: usize) {
    if settings.show_lines {
        print!("{:1$}", result.lines, max_width);
    }
    if settings.show_words {
        print!("{:1$}", result.words, max_width);
    }
    if settings.show_bytes {
        print!("{:1$}", result.bytes, max_width);
    }
    if settings.show_chars {
        print!("{:1$}", result.chars, max_width);
    }
    if settings.show_max_line_length {
        print!("{:1$}", result.max_line_length, max_width);
    }

    if result.title != "-" {
        println!(" {}", result.title);
    }
    else {
        println!("");
    }
}

fn open(path: &str) -> StdResult<BufReader<Box<Read+'static>>, i32> {
    if "-" == path {
        let reader = Box::new(stdin()) as Box<Read>;
        return Ok(BufReader::new(reader));
    }

    let fpath = Path::new(path);
    if fpath.is_dir() {
        show_info!("{}: is a directory", path);
    }
    match File::open(&fpath) {
        Ok(fd) => {
            let reader = Box::new(fd) as Box<Read>;
            Ok(BufReader::new(reader))
        }
        Err(e) => {
            show_error!("wc: {}: {}", path, e);
            Err(1)
        }
    }
}
