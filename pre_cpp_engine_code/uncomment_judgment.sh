#!/bin/sh
# engine.cpp의 판단 코드 블록([JUDGMENT-BEGIN 이름] ~ [JUDGMENT-END 이름])을 기계적으로 살린다.
# 블록 안의 각 줄에서 맨 앞 "// "(들여쓰기는 유지)만 벗기고, 그 밖의 줄은 그대로 출력한다.
# 리뷰어가 손으로 주석 기호만 지우는 것과 똑같은 변환이다. (표기 규칙은 engine.cpp 상단 참고)
#
# 사용법:
#   ./uncomment_judgment.sh engine.cpp > engine_all.cpp            # 모든 판단 블록을 살림
#   ./uncomment_judgment.sh engine.cpp C-shift C-jump > part.cpp   # 이름을 준 블록만 살림
#
# 블록 안에 "//"로 시작하지 않는 줄이 있으면 표기 규칙 위반이므로 오류로 끝낸다.

if [ $# -lt 1 ]; then
    echo "usage: $0 engine.cpp [block-name ...]" >&2
    exit 2
fi

file="$1"
shift
names="$*"

awk -v names="$names" '
BEGIN {
    n = split(names, list, " ")
    for (i = 1; i <= n; i++) {
        wanted[list[i]] = 1
    }
    inBlock = 0
    live = 0
}

/^[ \t]*\/\/ \[JUDGMENT-BEGIN [^ \]]+\]/ {
    name = $0
    sub(/^.*\[JUDGMENT-BEGIN +/, "", name)
    sub(/\].*/, "", name)
    inBlock = 1
    live = (n == 0) || (name in wanted)
    print
    next
}

/^[ \t]*\/\/ \[JUDGMENT-END [^ \]]+\]/ {
    inBlock = 0
    live = 0
    print
    next
}

inBlock && live {
    line = $0
    sub(/\r$/, "", line)
    match(line, /^[ \t]*/)
    indent = substr(line, 1, RLENGTH)
    rest = substr(line, RLENGTH + 1)

    if (rest !~ /^\/\//) {
        printf("%s:%d: 판단 블록 안의 줄이 // 로 시작하지 않음: %s\n", FILENAME, NR, line) > "/dev/stderr"
        bad = 1
    }

    sub(/^\/\/ ?/, "", rest)
    print indent rest
    next
}

{ print }

END {
    if (bad) {
        exit 1
    }
}
' "$file"
