# makefile for harness
DEBUG_OUT =
#DEBUG_OUT = -DONIG_DEBUG_PARSE -DONIG_DEBUG_COMPILE
#DEBUG_OUT = -DONIG_DEBUG_PARSE -DONIG_DEBUG_COMPILE -DONIG_DEBUG_MATCH_COUNTER

SRC = ../src
CFLAGS   = -I$(SRC) -Wall -g -fsanitize=fuzzer,address -fno-omit-frame-pointer
CFLAGS_M = -I$(SRC) -Wall -g -fsanitize=fuzzer-no-link,address -fno-omit-frame-pointer -DSTANDALONE
ONIG_LIB = $(SRC)/.libs/libonig.a
LIBS     = $(ONIG_LIB)

TARGETS = fuzzer-encode fuzzer-syntax fuzzer-utf16-be fuzzer-utf16-le \
          fuzzer-regset \
          read-encode read-syntax read-utf16-be read-utf16-le read-regset

OTHER_TARGETS = libfuzzer-onig libfuzzer-onig-full fuzzer-deluxe read-deluxe


#default: $(TARGETS)
default: read-syntax

fuzzer-encode: base.c $(ONIG_LIB)
	clang $(CFLAGS) $< $(LIBS) -o $@

fuzzer-syntax: base.c $(ONIG_LIB)
	clang -DSYNTAX_TEST $(CFLAGS) $< $(LIBS) -o $@

fuzzer-deluxe: deluxe.c $(ONIG_LIB)
	clang $(CFLAGS) $< $(LIBS) -o $@

fuzzer-utf16-be: base.c $(ONIG_LIB)
	clang -DUTF16_BE $(CFLAGS) $< $(LIBS) -o $@

fuzzer-utf16-le: base.c $(ONIG_LIB)
	clang -DUTF16_LE $(CFLAGS) $< $(LIBS) -o $@

fuzzer-regset: regset.c $(ONIG_LIB)
	clang $(CFLAGS) $< $(LIBS) -o $@

read-encode: base.c $(ONIG_LIB)
	clang $(CFLAGS_M) $< $(LIBS) -o $@

read-syntax: base.c $(ONIG_LIB)
	clang -DSYNTAX_TEST $(CFLAGS_M) $< $(LIBS) -o $@

read-deluxe: deluxe.c $(ONIG_LIB)
	clang $(CFLAGS_M) $< $(LIBS) -o $@

read-utf16-be: base.c $(ONIG_LIB)
	clang -DUTF16_BE $(CFLAGS_M) $< $(LIBS) -o $@

read-utf16-le: base.c $(ONIG_LIB)
	clang -DUTF16_LE $(CFLAGS_M) $< $(LIBS) -o $@

read-regset: regset.c $(ONIG_LIB)
	clang $(CFLAGS_M) $< $(LIBS) -o $@

libfuzzer-onig: libfuzzer-onig.cpp $(ONIG_LIB)
	clang++ $(CFLAGS) $< $(LIBS) -o $@

libfuzzer-onig-full: libfuzzer-onig.cpp $(ONIG_LIB)
	clang++ -DFULL_TEST $(CFLAGS) $< $(LIBS) -o $@


$(ONIG_LIB):
	cd ..; make clean
	#cd ..; autoreconf -vfi
	cd ..; ./configure CC=clang LD=clang CFLAGS="-g -fsanitize=address -fno-omit-frame-pointer $(DEBUG_OUT)" LDFLAGS="-g -fsanitize=address -fno-omit-frame-pointer"
	cd ..; make -j4


clean:
	rm -f $(TARGETS) $(OTHER_TARGETS)
