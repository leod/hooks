#!/bin/bash

n=$1
mode=$2

mkdir -p log

for i in `seq 1 $n`
do
  echo $i
  RUST_BACKTRACE=full RUST_LOG=info target/$mode/examples/random_bot &> log/random_bot_$i.log &
done
