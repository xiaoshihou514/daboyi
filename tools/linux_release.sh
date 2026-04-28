#! /usr/bin/env bash
set -ex

tmp=`mktemp --directory`
out="linux-x86_64.zip"

mkdir $tmp/bin
cp target/release/client $tmp/bin
cp -r assets $tmp
pushd $tmp
zip $out assets bin
popd
mv $tmp/$out .
