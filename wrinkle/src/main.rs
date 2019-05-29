extern crate regex;
extern crate tesseract;
#[macro_use]
extern crate lazy_static;
extern crate colored;
extern crate walkdir;
extern crate whatlang;

use colored::*;
use regex::Regex;
use tesseract::*;
use walkdir::WalkDir;
use whatlang::Lang;

use std::{
    cmp, env,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::Path,
};

fn main() {
    let mut args = env::args().skip(1);

    let sample = args.next().expect("no param 1");
    let target_dir = args.next().expect("no param 1");

    scan_dir(sample, target_dir);
}

static INCLUDED_EXTS: &[&'static str] = &["jpg", "jpeg", "png"];

pub fn is_image_file<P: AsRef<Path>>(path: P) -> bool {
    match path.as_ref().extension().and_then(|a| a.to_str()) {
        Some(ext) => INCLUDED_EXTS.contains(&ext),
        None => false,
    }
}

/**
 * `levenshtein-rs` - levenshtein
 *
 * MIT licensed.
 *
 * Copyright (c) 2016 Titus Wormer <tituswormer@gmail.com>
 */
pub fn levenshtein(a: &str, b: &str) -> usize {
    let mut result = 0;

    /* Shortcut optimizations / degenerate cases. */
    if a == b {
        return result;
    }
    
    let length_a = a.chars().count();
    let length_b = b.chars().count();

    if length_a == 0 {
        return length_b;
    }

    if length_b == 0 {
        return length_a;
    }

    /* Initialize the vector.
     *
     * This is why it’s fast, normally a matrix is used,
     * here we use a single vector. */
    let mut cache: Vec<usize> = vec![0; length_a];
    let mut index_a = 0;
    let mut distance_a;
    let mut distance_b;

    while index_a < length_a {
        index_a += 1;
        cache[index_a - 1] = index_a;
    }

    /* Loop. */
    for (index_b, code_b) in b.chars().enumerate() {
        result = index_b;
        distance_a = index_b;

        for (index_a, code_a) in a.chars().enumerate() {
            distance_b = if code_a == code_b {
                distance_a
            } else {
                distance_a + 1
            };

            distance_a = cache[index_a];

            result = if distance_a > result {
                if distance_b > result {
                    result + 1
                } else {
                    distance_b
                }
            } else {
                if distance_b > distance_a {
                    distance_a + 1
                } else {
                    distance_b
                }
            };

            cache[index_a] = result;
        }
    }

    result
}

pub fn scan_dir<P: AsRef<Path>>(sample: P, target_dir: P) {
    let text = extract_text(sample);
    println!("sample text: {}", text);
    let bs = text_to_bin(&text);
    let sample_bin = &bs.bin;
    let sample_size = sample_bin.len();
    let max_size = sample_size;

    // dbg!(&bs);

    println!("max_size: {}", max_size);

    if max_size == 0 {
        return;
    }

    let sample_bin: Vec<u8> = bs.bin.iter().take(max_size).map(|a| *a).collect();

    println!("{}", format!("scanning {}", target_dir.as_ref().display()).color("grey"));

    for entry in WalkDir::new(target_dir)
        .into_iter()
        .filter_entry(|e| e.path().is_dir() || is_image_file(e.path()))
    {
        let entry = match entry {
            Ok(et) => et,
            Err(e) => {
                println!("Cannot open {}", e);
                continue;
            }
        };

        if entry.path().is_dir() {
            continue;
        }

        print!("{}", format!("SCAN: {}                                               ", entry.path().display()).color("grey"));
        print!("\r");
        std::io::stdout().flush();

        let text = extract_text(entry.path());
        let bs = text_to_bin(&text);
        if bs.bin.len() < 16 {
            continue;
        }
        let max_size = cmp::min(bs.bin.len(), sample_size);

        let mut soffset = 0;
        let mut found = false;

        for i in 0..bs.bin.len() {
            // dbg!((&bin[i..max_size], &sample_bin[0..10]));
            if i + sample_size > bs.bin.len() {
                break;
            }
            let lv_distance = levenshtein(std::str::from_utf8(&bs.bin[i..i + max_size]).unwrap(), std::str::from_utf8(&sample_bin[0..max_size]).unwrap());
            // dbg!(lv_distance);

            // if &bs.bin[i..i + 32] == &sample_bin[0..32] {
            if lv_distance < 5 {
                dbg!(lv_distance);
                println!("{:?} == {:?}", &bs.bin[i..i + max_size], &sample_bin[0..max_size]);
                soffset = i;
                found = true;
                break;
            }
        }

        if found {
            println!(
                "{}",
                format!(
                    "{}",
                    entry.path().display()
                )
                .yellow()
            );
        }
    }
}

lazy_static! {
    static ref NOISE_WORDS: Regex = Regex::new(
        "\\b(\
         [aeueo]{2}|\
         (Senin|Selasa|Rabu|Kamis|Jum'at|Sabtu|Minggu),? \\d\\d? .*|\
         elo|\
         [=\",\\.,aeueo_-]{2}|\
         ([a-zA-Z0-9]{2} [a-zA-Z0-9]{2})+
         )\\b"
    )
    .unwrap();
    static ref NORM_WORD: Regex = Regex::new("[\\('\",.]?([a-zA-Z0-9]*)(\\.+)?(!+)?[”\\)',.]?").unwrap();
    static ref LONG_SPACE: Regex = Regex::new("\\s\\s+").unwrap();
    static ref WORDLIST: Vec<Vec<u8>> = {
        let file = File::open("indonesian.lst").expect("cannot read indonesian.lst file");
        let mut rvs = vec![];
        for line in BufReader::new(file).lines() {
            if let Some(line) = line.ok() {
                rvs.push(line.into_bytes());
            }
        }
        rvs
    };
}

fn extract_text<P: AsRef<Path>>(path: P) -> String {
    let cube = Tesseract::new();

    cube.set_lang("ind");
    cube.set_image(&path.as_ref().to_string_lossy().to_string());
    cube.set_variable("save_best_choices", "T");
    cube.recognize();

    cleanup_text(&cube.get_text().to_string())
}

fn cleanup_text(text: &str) -> String {
    let rv = NOISE_WORDS.replace_all(text, "").to_string();
    let text_lines = rv.split("\n");
    let mut rvs: Vec<String> = vec![];
    for line in text_lines {
        let info = whatlang::detect(&line);

        if info.is_none() {
            continue;
        }

        let lang = info.unwrap().lang();

        // dbg!((&lang, &line));

        if !(lang == Lang::Ind || lang == Lang::Ilo || lang == Lang::Pol) {
            continue;
        }

        let s: Vec<&str> = line.split(" ").collect();
        let mut ws = vec![];
        for w in s {
            let w = NORM_WORD.replace_all(w, "$1");
            // println!("w: {}", w);
            ws.push(w.to_string());
        }
        rvs.push(ws.join(" "));
    }
    LONG_SPACE.replace_all(&rvs.join("\n"), " ").to_string()
}

#[derive(Debug)]
pub struct TextBinSeq {
    pub texts: Vec<String>,
    pub bin: Vec<u8>,
}

fn text_to_bin(text: &str) -> TextBinSeq {
    let mut texts: Vec<String> = vec![];
    let mut bin: Vec<u8> = vec![];

    let text_lines = text.split("\n");

    for line in text_lines {
        let s: Vec<&str> = line.split(" ").collect();
        let mut ws = vec![];
        let mut has_words = vec![];

        for w in &s {
            // dbg!(w.to_lowercase().as_bytes().to_vec());
            let w2 = w.to_lowercase().as_bytes().to_vec();
            if w2.len() < 1 {
                continue;
            }

            if WORDLIST.contains(&w2) {
                has_words.push(1);
            } else {
                has_words.push(0);
            }
        }

        let wth: u64 = has_words
            .iter()
            .map(|a| *a as u64)
            .enumerate()
            .fold(0, |v, (i, b)| v | (b << i));

        // dbg!((wth, has_words, &line));

        if wth > 2 {
            for w in s {
                for c in w.chars() {
                    let c = c as u8;
                    // dbg!(c);
                    if c == 97
                        || c == 101
                        || c == 105
                        || c == 117
                        || c == 101
                        || c == 111
                        || c == 32
                    {
                        ws.push(0);
                    } else if c == 65 || c == 69 || c == 85 || c == 73 || c == 79 {
                        ws.push(1);
                    } else if c >= 97 && c <= 122 {
                        ws.push(2);
                    } else if c >= 65 && c <= 90 {
                        ws.push(3);
                    } else if c >= 48 && c <= 57 { // numeric
                        ws.push(4);
                    } else {
                        ws.push(0);
                    }
                }
            }
        }

        let th: u64 = ws
            .iter()
            .map(|a| *a as u64)
            .enumerate()
            .fold(0, |v, (i, b)| v | (b << i));

        // dbg!((&th, &line, &ws));

        if th > 100 {
            texts.push(line.to_owned());
            bin.append(&mut ws);
        }
    }
    
    TextBinSeq { texts, bin }
}

fn hamming_distance(a: u64, b: u64) -> u64 {
    let mut h = 0;
    let mut d = a ^ b;
    while d > 0 {
        h += 1;
        d &= d - 1
    }
    h
}
