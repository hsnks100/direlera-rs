#!/bin/bash

git pull
# build direlera-rs
# find direlera-rs and kill it.
kill $(ps aux | grep direlera-rs | grep -v grep | awk '{print $2}')
# start direlera-rs as daemon
nohup cargo run --release &