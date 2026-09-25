이 폴더는 rust를 다루지 못하는 한 불쌍한 개발자가 cpp로 손코딩하고 그걸 ai한테 rust포팅 요청을 날리면서 제작하기 위한 작은 스케치 코드입니다.

## 판단 코드 표기 규칙

규칙이 애매해서 판단이 필요한 곳은 설명만 남기지 않고, 실제로 동작하는 코드를 짜서 주석 처리해 두었습니다.
형식은 `// 판단(이름): 한 줄 근거` 아래에 `// [JUDGMENT-BEGIN 이름]` ~ `// [JUDGMENT-END 이름]` 블록이 오고,
블록 안의 각 줄 맨 앞 `// `만 지우면 그대로 살아납니다. 같은 이름의 블록은 함께 살려야 합니다.

- 전부 살린 변형본: `./uncomment_judgment.sh engine.cpp > engine_all.cpp`
- 이름을 지정해 일부만: `./uncomment_judgment.sh engine.cpp C-shift C-jump > part.cpp`
- 빌드와 자체 검사 실행: `g++ -std=c++20 -Wall -Wextra engine.cpp -o engine_test && ./engine_test`
  (`main()`이 검사를 돌리고 실패하면 0이 아닌 값을 반환합니다. 변형본도 같은 방식으로 빌드합니다.)
