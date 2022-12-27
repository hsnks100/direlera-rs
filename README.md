# direlera-rs

This is a project to rewrite kaillera server in rust.

I've had a lot of memories with Netplay from before, but I wanted to try making a Kaillera server someday. server name is 'Direlera'

The orientation of the server is as follows.
1. Aim for fast response speed.
2. Eliminate operationally unstable factors such as server crashes.

I have a job and I can't spend all my time here, so only basic functions are still available and there are no convenience functions.

# build

## linux / mac

```bash
curl https://sh.rustup.rs -sSf | sh -s -- --help
https://github.com/hsnks100/direlera-rs.git
cd direlera-rs
cargo build # or cargo run --release
```

## windows 

install rust  
link: https://forge.rust-lang.org/infra/other-installation-methods.html



```
cargo run --release # in project 
```