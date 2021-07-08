# Try some profile guided optimiztion
# https://doc.rust-lang.org/rustc/profile-guided-optimization.html
# rustup component add llvm-tools-preview
set -x

# STEP 0: Make sure there is no left-over profiling data from previous runs
cwd=$(pwd)
rm -rf "$cwd"/pgo-data
mkdir "$cwd"/pgo-data

# Get 1_000_000 lines of wide csv file
wget -q -S -O - https://archive.ics.uci.edu/ml/machine-learning-databases/00347/all_train.csv.gz \
    | gunzip \
    | head -n 1000000 \
    > "$cwd"/pgo-data/data.csv

data="./pgo-data/data.csv"
spaced_data="./pgo-data/spaced_data.txt"
llvm_profdata=$(find ~/.rustup/toolchains/stable-*/lib/rustlib/*/bin/ -name llvm-profdata -type f)

# STEP 1: Build the instrumented binaries
RUSTFLAGS="-Cprofile-generate=$cwd/pgo-data" \
    cargo build --release

# STEP 2: Run the instrumented binaries with some typical data

# single-byte in order
./target/release/hck -Ld, -f1,8,19 "$data"  > /dev/null
./target/release/hck -Ld, --no-mmap -f1,8,19 "$data"  > /dev/null

# single byte reorder
./target/release/hck -Ld, -f1,19,8 "$data" > /dev/null
./target/release/hck -Ld, --no-mmap -f1,19,8 "$data" > /dev/null

# single byte regex
./target/release/hck -d, -f1,8,19 "$data" > /dev/null
./target/release/hck -d, -f1,8,19 --no-mmap "$data" > /dev/null

# make multi-space file
./target/release/hck -Ld, -D '   ' -f1,8,19 "$data" > "$spaced_data"

# multi byte regex
./target/release/hck -d '\s+' -f1,8,19 "$spaced_data" > /dev/null
./target/release/hck -d '\s+' --no-mmap -f1,8,19 "$spaced_data" > /dev/null

rm "$spaced_data"
rm "$data"

# STEP 3: Merge the `.profraw` files into a `.profdata` file
"$llvm_profdata" merge -o "$cwd"/pgo-data/merged.profdata "$cwd"/pgo-data

# STEP 4: Use the `.profdata` file for guiding optimizations
RUSTFLAGS="-Cllvm-args=-pgo-warn-missing-function -Cprofile-use=$cwd/pgo-data/merged.profdata" \
    cargo build --release
