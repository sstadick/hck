

llvm_profdata := `find $(rustc --print sysroot) -name llvm-profdata -type f`

pgo-data := join(justfile_directory(), "pgo-data")
data := join(pgo-data, "data.csv")
spaced-data := join(pgo-data, "spaced_data.txt")
gzip-data := join(pgo-data, "data.csv.gz")

prep:
    rustup component add llvm-tools-preview
    mkdir -p {{pgo-data}}

clean:
    rm -rf {{pgo-data}}

get-data: prep
    wget -q -S -O - https://archive.ics.uci.edu/ml/machine-learning-databases/00347/all_train.csv.gz \
        | gunzip \
        | head -n 1000000 \
        > {{pgo-data}}/data.csv

generate-pgo-data: prep get-data
    RUSTFLAGS="-Cprofile-generate={{pgo-data}}" \
        cargo build --release --locked

run-instrumented-binary: generate-pgo-data
    # single-byte in order
    {{justfile_directory()}}/target/release/hck -Ld, -f1,8,19 -e 8 "{{data}}"  > /dev/null
    {{justfile_directory()}}/target/release/hck -Ld, --no-mmap -f1,8,19 -e 8 "{{data}}"  > /dev/null


    # single byte reorder
    {{justfile_directory()}}/target/release/hck -Ld, -f1,19,8 -e 8 "{{data}}" > /dev/null
    {{justfile_directory()}}/target/release/hck -Ld, --no-mmap -f1,19,8 -e 8 "{{data}}" > /dev/null

    # single byte regex
    {{justfile_directory()}}/target/release/hck -d, -f1,8,19 -e 8 "{{data}}" > /dev/null
    {{justfile_directory()}}/target/release/hck -d, -f1,8,19 --no-mmap -e 8 "{{data}}" > /dev/null

    # make multi-space file
    {{justfile_directory()}}/target/release/hck -Ld, -D '   ' -f1,8,19 -e 8 "{{data}}" > "{{spaced-data}}"

    # multi byte regex
    {{justfile_directory()}}/target/release/hck -d '\s+' -f1,8,19 -e 8 "{{spaced-data}}" > /dev/null
    {{justfile_directory()}}/target/release/hck -d '\s+' --no-mmap -f1,8,19 -e 8 "{{spaced-data}}" > /dev/null
    {{justfile_directory()}}/target/release/hck -d '[[:space:]]+' -f1,8,19 -e 8 "{{spaced-data}}" > /dev/null
    {{justfile_directory()}}/target/release/hck -d '[[:space:]]+' --no-mmap -f1,8,19 -e 8 "{{spaced-data}}" > /dev/null

    # gzip decoding
    gzip -c "{{data}}" > "{{gzip-data}}"
    {{justfile_directory()}}/target/release/hck -Z -Ld, -D, -f1- -e 8 "{{data}}" > "{{gzip-data}}"
    {{justfile_directory()}}/target/release/hck -z -Ld, -f1,19,8 -e 8 "{{gzip-data}}" > /dev/null

clean-data: run-instrumented-binary
    rm {{data}}
    rm {{spaced-data}}
    rm {{gzip-data}}


merge-prof-data llvm-profdata=`find ~/.rustup/toolchains/stable-*/lib/rustlib/*/bin/ -name llvm-profdata -type f`: clean-data prep
    @echo "Using llvm-profdata: {{llvm-profdata}}"
    {{llvm-profdata}} merge -o {{pgo-data}}/merged.profdata {{pgo-data}}

build: merge-prof-data
    RUSTFLAGS="-Cllvm-args=-pgo-warn-missing-function -Cprofile-use={{pgo-data}}/merged.profdata" \
        cargo build --release --locked

build-native: merge-prof-data
    RUSTFLAGS="-Ctarget-cpu=native -Cllvm-args=-pgo-warn-missing-function -Cprofile-use={{pgo-data}}/merged.profdata" \
        cargo build --release --locked

install-native: build-native
    cp {{justfile_directory()}}/target/release/hck ~/.cargo/bin/hck
