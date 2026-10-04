use rustc_hash::FxHashMap;
use std::{hint::black_box, time::Instant};

fn linear(batches: &[Vec<String>]) -> Vec<String> {
    let mut names = Vec::new();
    for batch in batches {
        let own = batch.clone();
        for name in own {
            if !names.contains(&name) { names.push(name); }
        }
    }
    names
}

fn indexed(batches: &[Vec<String>]) -> Vec<String> {
    let mut index = FxHashMap::default();
    for batch in batches {
        let own = batch.clone();
        for name in own {
            let next = index.len();
            index.entry(name).or_insert(next);
        }
    }
    let mut names = vec![String::new(); index.len()];
    for (name, position) in index { names[position] = name; }
    names
}

fn hybrid(batches: &[Vec<String>]) -> Vec<String> {
    let mut names=Vec::new();
    let mut index:Option<FxHashMap<String,usize>>=None;
    for batch in batches {
        let own=batch.clone();
        for name in own {
            if index.is_none() && names.len()==32 {
                index=Some(std::mem::take(&mut names).into_iter().enumerate().map(|(i,n)|(n,i)).collect());
            }
            if let Some(index)=&mut index {
                let next=index.len(); index.entry(name).or_insert(next);
            } else if !names.contains(&name) { names.push(name); }
        }
    }
    if let Some(index)=index {
        names=vec![String::new();index.len()];
        for (name,position) in index { names[position]=name; }
    }
    names
}

fn main() {
    let cases = [("small",4,1), ("small-duplicate",4,4), ("medium",32,1),
                 ("medium-duplicate",32,4), ("wide",256,1), ("wide-duplicate",256,4),
                 ("huge-duplicate",1024,4)];
    for (name,size,bases) in cases {
        let batches:Vec<Vec<String>>=(0..bases).map(|base| (0..size).map(|i|
            format!("property_{}",if base%2==0 {i} else {size-i-1})).collect()).collect();
        assert_eq!(linear(&batches),indexed(&batches));
        assert_eq!(linear(&batches),hybrid(&batches));
        let loops=200_000/(size*bases).max(1);
        for round in 0..5 {
            for mode in if round%2==0 { ["linear","indexed","hybrid"] } else { ["hybrid","indexed","linear"] } {
                let start=Instant::now();
                for _ in 0..loops {
                    black_box(match mode { "linear"=>linear(black_box(&batches)),"hybrid"=>hybrid(black_box(&batches)),_=>indexed(black_box(&batches)) });
                }
                println!("{{\"case\":\"{name}\",\"size\":{size},\"bases\":{bases},\"round\":{round},\"mode\":\"{mode}\",\"loops\":{loops},\"ns\":{}}}",start.elapsed().as_nanos());
            }
        }
    }
}
