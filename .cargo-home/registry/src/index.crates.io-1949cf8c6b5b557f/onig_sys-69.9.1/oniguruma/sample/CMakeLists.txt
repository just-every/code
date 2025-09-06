cmake_minimum_required(VERSION 2.8)
project(oniguruma_sample C)

add_executable(crnl crnl.c)
target_link_libraries(crnl onig)

add_executable(callout callout.c)
target_link_libraries(callout onig)

add_executable(echo echo.c)
target_link_libraries(echo onig)

add_executable(count count.c)
target_link_libraries(count onig)

add_executable(encode encode.c)
target_link_libraries(encode onig)

add_executable(listcap listcap.c)
target_link_libraries(listcap onig)

add_executable(names names.c)
target_link_libraries(names onig)

add_executable(posix posix.c)
target_link_libraries(posix onig)

add_executable(simple simple.c)
target_link_libraries(simple onig)

add_executable(sql sql.c)
target_link_libraries(sql onig)

add_executable(syntax syntax.c)
target_link_libraries(syntax onig)
