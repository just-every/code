/*
 * base.c  contributed by Mark Griffin
 * Copyright (c) 2019-2022  K.Kosako
 */
#include <stdio.h>
#include <unistd.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <time.h>
#include "oniguruma.h"

#define PARSE_DEPTH_LIMIT               8
#define MAX_SUBEXP_CALL_NEST_LEVEL      8
#define SUBEXP_CALL_LIMIT            1000
#define BASE_RETRY_LIMIT            20000
#define BASE_LENGTH                  2048
#define MATCH_STACK_LIMIT        10000000
#define MAX_REM_SIZE              1048576
#define MAX_SLOW_REM_SIZE            1024
#define MAX_SLOW_REM_SIZE2             80
#define SLOW_RETRY_LIMIT             2000
#define SLOW_SUBEXP_CALL_LIMIT        100
#define MAX_SLOW_BACKWARD_REM_SIZE    200

//#define EXEC_PRINT_INTERVAL      500000
//#define DUMP_DATA_INTERVAL       100000
//#define STAT_PATH                "fuzzer.stat_log"
//#define PREV_CONTROL

#ifdef PREV_CONTROL
#define OPTIONS_AT_COMPILE   (ONIG_OPTION_IGNORECASE | ONIG_OPTION_EXTEND | ONIG_OPTION_MULTILINE | ONIG_OPTION_SINGLELINE | ONIG_OPTION_FIND_LONGEST | ONIG_OPTION_FIND_NOT_EMPTY | ONIG_OPTION_NEGATE_SINGLELINE | ONIG_OPTION_DONT_CAPTURE_GROUP | ONIG_OPTION_CAPTURE_GROUP | ONIG_OPTION_WORD_IS_ASCII | ONIG_OPTION_DIGIT_IS_ASCII | ONIG_OPTION_SPACE_IS_ASCII | ONIG_OPTION_POSIX_IS_ASCII | ONIG_OPTION_TEXT_SEGMENT_EXTENDED_GRAPHEME_CLUSTER | ONIG_OPTION_TEXT_SEGMENT_WORD)
#else
#define OPTIONS_AT_COMPILE   (ONIG_OPTION_IGNORECASE | ONIG_OPTION_EXTEND | ONIG_OPTION_MULTILINE | ONIG_OPTION_SINGLELINE | ONIG_OPTION_FIND_LONGEST | ONIG_OPTION_FIND_NOT_EMPTY | ONIG_OPTION_NEGATE_SINGLELINE | ONIG_OPTION_DONT_CAPTURE_GROUP | ONIG_OPTION_CAPTURE_GROUP | ONIG_OPTION_WORD_IS_ASCII | ONIG_OPTION_DIGIT_IS_ASCII | ONIG_OPTION_SPACE_IS_ASCII | ONIG_OPTION_POSIX_IS_ASCII | ONIG_OPTION_TEXT_SEGMENT_EXTENDED_GRAPHEME_CLUSTER | ONIG_OPTION_TEXT_SEGMENT_WORD | ONIG_OPTION_IGNORECASE_IS_ASCII)
#endif

#define OPTIONS_AT_RUNTIME   (ONIG_OPTION_NOTBOL | ONIG_OPTION_NOTEOL | ONIG_OPTION_CHECK_VALIDITY_OF_STRING | ONIG_OPTION_NOT_BEGIN_STRING | ONIG_OPTION_NOT_END_STRING | ONIG_OPTION_NOT_BEGIN_POSITION | ONIG_OPTION_CALLBACK_EACH_MATCH)


#define ADJUST_LEN(enc, len) do {\
  int mlen = ONIGENC_MBC_MINLEN(enc);\
  if (mlen != 1) { len -= len % mlen; }\
} while (0)

typedef unsigned char uint8_t;


//#define TEST_PATTERN

#ifdef TEST_PATTERN

#if 1
unsigned char TestPattern[] = {
};
#endif

#endif /* TEST_PATTERN */

#ifdef STANDALONE

static void
print_options(FILE* fp, OnigOptionType o)
{
  if ((o & ONIG_OPTION_IGNORECASE) != 0)      fprintf(fp, " IGNORECASE");
  if ((o & ONIG_OPTION_EXTEND) != 0)          fprintf(fp, " EXTEND");
  if ((o & ONIG_OPTION_MULTILINE) != 0)       fprintf(fp, " MULTILINE");
  if ((o & ONIG_OPTION_SINGLELINE) != 0)      fprintf(fp, " SINGLELINE");
  if ((o & ONIG_OPTION_FIND_LONGEST) != 0)    fprintf(fp, " FIND_LONGEST");
  if ((o & ONIG_OPTION_FIND_NOT_EMPTY) != 0)  fprintf(fp, " FIND_NOT_EMPTY");
  if ((o & ONIG_OPTION_NEGATE_SINGLELINE) != 0)  fprintf(fp, " NEGATE_SINGLELINE");
  if ((o & ONIG_OPTION_DONT_CAPTURE_GROUP) != 0) fprintf(fp, " DONT_CAPTURE_GROUP");
  if ((o & ONIG_OPTION_CAPTURE_GROUP) != 0)   fprintf(fp, " CAPTURE_GROUP");
  if ((o & ONIG_OPTION_NOTBOL) != 0)          fprintf(fp, " NOTBOL");
  if ((o & ONIG_OPTION_NOTEOL) != 0)          fprintf(fp, " NOTEOL");
  if ((o & ONIG_OPTION_POSIX_REGION) != 0)    fprintf(fp, " POSIX_REGION");
  if ((o & ONIG_OPTION_CHECK_VALIDITY_OF_STRING) != 0) fprintf(fp, " CHECK_VALIDITY_OF_STRING");
  if ((o & ONIG_OPTION_IGNORECASE_IS_ASCII) != 0) fprintf(fp, " IGNORECASE_IS_ASCII");
  if ((o & ONIG_OPTION_WORD_IS_ASCII) != 0)   fprintf(fp, " WORD_IS_ASCII");
  if ((o & ONIG_OPTION_DIGIT_IS_ASCII) != 0)  fprintf(fp, " DIGIT_IS_ASCII");
  if ((o & ONIG_OPTION_SPACE_IS_ASCII) != 0)  fprintf(fp, " SPACE_IS_ASCII");
  if ((o & ONIG_OPTION_POSIX_IS_ASCII) != 0)  fprintf(fp, " POSIX_IS_ASCII");
  if ((o & ONIG_OPTION_TEXT_SEGMENT_EXTENDED_GRAPHEME_CLUSTER) != 0) fprintf(fp, " TEXT_SEGMENT_EXTENDED_GRAPHEME_CLUSTER");
  if ((o & ONIG_OPTION_TEXT_SEGMENT_WORD) != 0) fprintf(fp, " TEXT_SEGMENT_WORD");
  if ((o & ONIG_OPTION_NOT_BEGIN_STRING) != 0) fprintf(fp, " NOT_BIGIN_STRING");
  if ((o & ONIG_OPTION_NOT_END_STRING) != 0)   fprintf(fp, " NOT_END_STRING");
  if ((o & ONIG_OPTION_NOT_BEGIN_POSITION) != 0) fprintf(fp, " NOT_BEGIN_POSITION");
  if ((o & ONIG_OPTION_CALLBACK_EACH_MATCH) != 0) fprintf(fp, " CALLBACK_EACH_MATCH");
}

static void
to_binary(unsigned int v, char s[/* 33 */])
{
  unsigned int mask;
  int i;

  mask = 1 << (sizeof(v) * 8 - 1);
  i = 0;
  do {
    s[i++] = (mask & v ? '1' : '0');
  } while (mask >>= 1);
  s[i] = 0;
}
#endif

#ifdef DUMP_INPUT
static void
dump_input(unsigned char* data, size_t len)
{
  static FILE* DumpFp;
  static char end[] = { 'E', 'N', 'D' };

  if (DumpFp == 0)
    DumpFp = fopen("dump-input", "w");

  fseek(DumpFp, 0, SEEK_SET);
  fwrite(data, sizeof(unsigned char), len, DumpFp);
  fwrite(end,  sizeof(char), sizeof(end), DumpFp);
  fflush(DumpFp);
}
#endif

#ifdef DUMP_DATA_INTERVAL
static void
dump_file(char* path, unsigned char* data, size_t len)
{
  FILE* fp;

  fp = fopen(path, "w");
  fwrite(data, sizeof(unsigned char), len, fp);
  fclose(fp);
}
#endif

#ifdef STANDALONE
#include <ctype.h>

static void
dump_data(FILE* fp, unsigned char* data, int len)
{
  int i;

  fprintf(fp, "{\n");
  for (i = 0; i < len; i++) {
    unsigned char c = data[i];

    if (isprint((int )c)) {
      if (c == '\\')
        fprintf(fp, " '\\\\'");
      else if (c == '\'')
        fprintf(fp, " '\\''");
      else
        fprintf(fp, " '%c'", c);
    }
    else {
      fprintf(fp, "0x%02x", (int )c);
    }

    if (i == len - 1) {
      fprintf(fp, "\n");
    }
    else {
      if (i % 8 == 7)
        fprintf(fp, ",\n");
      else
        fprintf(fp, ", ");
    }
  }
  fprintf(fp, "};\n");
}

#else

#ifdef EXEC_PRINT_INTERVAL
static void
output_current_time(FILE* fp)
{
  char d[64];
  time_t t;

  t = time(NULL);
  strftime(d, sizeof(d), "%m/%d %H:%M:%S", localtime(&t));

  fprintf(fp, "%s", d);
}
#endif

#endif

static int
progress_callout_func(OnigCalloutArgs* args, void* user_data)
{
  return ONIG_CALLOUT_SUCCESS;
}

static int
each_match_callback_func(const UChar* str, const UChar* end,
  const UChar* match_start, OnigRegion* region, void* user_data)
{
  return ONIG_NORMAL;
}

static unsigned int calc_retry_limit(int sl, int len)
{
  unsigned int r;
  unsigned int upper;
  int heavy;

  heavy = sl >> 8;
  sl &= 0xff;
  sl += heavy;

  upper = BASE_RETRY_LIMIT;
  if (sl == 2) {
    upper = SLOW_RETRY_LIMIT;
  }
  else if (sl > 2) {
    upper = SLOW_RETRY_LIMIT * 3 / sl;
    if (upper <= 10) upper = 10;
  }

  if (len < BASE_LENGTH) {
    r = BASE_RETRY_LIMIT;
  }
  else {
    r = BASE_RETRY_LIMIT * BASE_LENGTH / len;
  }

  if (r > upper)
    r = upper;

  return r;
}

static int
search(regex_t* reg, unsigned char* str, unsigned char* end, OnigOptionType options, int backward, int sl)
{
  int r;
  unsigned char *start, *range;
  OnigRegion *region;
  unsigned int retry_limit;
  size_t len;

  region = onig_region_new();

  len = (size_t )(end - str);
  retry_limit = calc_retry_limit(sl, len);

  onig_set_retry_limit_in_search(retry_limit);
  onig_set_match_stack_limit_size(MATCH_STACK_LIMIT);
  if (sl >= 2)
    onig_set_subexp_call_limit_in_search(SLOW_SUBEXP_CALL_LIMIT);
  else
    onig_set_subexp_call_limit_in_search(SUBEXP_CALL_LIMIT);

#ifdef STANDALONE
  fprintf(stdout, "retry limit: %u\n", retry_limit);
  fprintf(stdout, "end - str: %td\n", end - str);
#endif

  if (backward != 0) {
    start = end;
    range = str;
  }
  else {
    start = str;
    range = end;
  }

  r = onig_search(reg, str, end, start, range, region, (options & OPTIONS_AT_RUNTIME));
  if (r >= 0) {
#ifdef STANDALONE
    int i;

    fprintf(stdout, "match at %d  (%s)\n", r,
            ONIGENC_NAME(onig_get_encoding(reg)));
    for (i = 0; i < region->num_regs; i++) {
      fprintf(stdout, "%d: (%d-%d)\n", i, region->beg[i], region->end[i]);
    }
#endif
  }
  else if (r == ONIG_MISMATCH) {
#ifdef STANDALONE
    fprintf(stdout, "search fail (%s)\n",
            ONIGENC_NAME(onig_get_encoding(reg)));
#endif
  }
  else { /* error */
#ifdef STANDALONE
    char s[ONIG_MAX_ERROR_MESSAGE_LEN];

    onig_error_code_to_str((UChar* )s, r);
    fprintf(stdout, "ERROR: %s\n", s);
    fprintf(stdout, "  (%s)\n", ONIGENC_NAME(onig_get_encoding(reg)));
#endif
    onig_region_free(region, 1 /* 1:free self, 0:free contents only */);

    if (r == ONIGERR_STACK_BUG ||
        r == ONIGERR_UNDEFINED_BYTECODE ||
        r == ONIGERR_UNEXPECTED_BYTECODE)
      return -2;

    return -1;
  }

  onig_region_free(region, 1 /* 1:free self, 0:free contents only */);
  return 0;
}

static long INPUT_COUNT;
static long EXEC_COUNT;
static long EXEC_COUNT_INTERVAL;
static long REGEX_SUCCESS_COUNT;
static long VALID_STRING_COUNT;

static int
exec(OnigEncoding enc, OnigOptionType options, OnigSyntaxType* syntax,
     char* apattern, char* apattern_end,
     char* adata_pattern, char* adata_pattern_end,
     char* astr, UChar* end, int backward, int sl)
{
  int r;
  regex_t* reg;
  OnigErrorInfo einfo;
  UChar* str     = (UChar* )astr;
  UChar* pattern = (UChar* )apattern;
  UChar* pattern_end = (UChar* )apattern_end;
  UChar* data_pattern = (UChar* )adata_pattern;
  UChar* data_pattern_end = (UChar* )adata_pattern_end;

  EXEC_COUNT++;
  EXEC_COUNT_INTERVAL++;

  onig_initialize(&enc, 1);
  (void)onig_set_progress_callout(progress_callout_func);
#ifdef PARSE_DEPTH_LIMIT
  onig_set_parse_depth_limit(PARSE_DEPTH_LIMIT);
#endif
  onig_set_subexp_call_max_nest_level(MAX_SUBEXP_CALL_NEST_LEVEL);
  onig_set_callback_each_match(each_match_callback_func);

  r = onig_new(&reg, pattern, pattern_end,
               (options & OPTIONS_AT_COMPILE), enc, syntax, &einfo);
  if (r != ONIG_NORMAL) {
    char s[ONIG_MAX_ERROR_MESSAGE_LEN];
    onig_error_code_to_str((UChar* )s, r, &einfo);
#ifdef STANDALONE
    fprintf(stdout, "ERROR: %s\n", s);
#endif
    onig_end();

    if (r == ONIGERR_PARSER_BUG ||
        r == ONIGERR_STACK_BUG  ||
        r == ONIGERR_UNDEFINED_BYTECODE ||
        r == ONIGERR_UNEXPECTED_BYTECODE) {
      return -2;
    }
    else
      return -1;
  }
  REGEX_SUCCESS_COUNT++;

  if (data_pattern == pattern ||
      onigenc_is_valid_mbc_string(enc, data_pattern, data_pattern_end) != 0) {
    r = search(reg, data_pattern, data_pattern_end, options, backward, sl);
    if (r == -2) return -2;
  }

  if (onigenc_is_valid_mbc_string(enc, str, end) != 0) {
    VALID_STRING_COUNT++;
    r = search(reg, str, end, options, backward, sl);
    if (r == -2) return -2;
  }

  onig_free(reg);
  onig_end();
  return 0;
}

static size_t
fix_size(size_t x, OnigEncoding enc, int sl, int backward)
{
  if (x > MAX_REM_SIZE) x = MAX_REM_SIZE;

  if (sl > 0) {
    if (sl >= 256) { // 256: exists heavy element
      if (x > MAX_SLOW_REM_SIZE2) x = MAX_SLOW_REM_SIZE2;
    }
    else {
      if (x > MAX_SLOW_REM_SIZE) x = MAX_SLOW_REM_SIZE;
    }
  }
  if (backward != 0 && enc == ONIG_ENCODING_GB18030) {
    if (x > MAX_SLOW_BACKWARD_REM_SIZE)
      x = MAX_SLOW_BACKWARD_REM_SIZE;
  }

  ADJUST_LEN(enc, x);
  return x;
}

static int
alloc_exec(OnigEncoding enc, OnigOptionType options, OnigSyntaxType* syntax,
           int backward, int pattern_size, size_t rem_size, unsigned char *data)
{
  extern int onig_detect_can_be_slow_pattern(const UChar* pattern, const UChar* pattern_end, OnigOptionType option, OnigEncoding enc, OnigSyntaxType* syntax);

  int r;
  int sl;
  int data_pattern_size;
  unsigned char *pattern;
  unsigned char *pattern_end;
  unsigned char *data_pattern;
  unsigned char *data_pattern_end;
  unsigned char *str_null_end;

#ifdef TEST_PATTERN
  pattern = (unsigned char *)malloc(sizeof(TestPattern));
  memcpy(pattern, TestPattern, sizeof(TestPattern));
  pattern_end = pattern + sizeof(TestPattern);
#else
  pattern = (unsigned char *)malloc(pattern_size != 0 ? pattern_size : 1);
  pattern_end = pattern + pattern_size;
  memcpy(pattern, data, pattern_size);
#endif

  sl = onig_detect_can_be_slow_pattern(pattern, pattern_end, options, enc, syntax);
#ifdef STANDALONE
  fprintf(stdout, "sl: %d\n", sl);
#endif

  data_pattern_size = fix_size(pattern_size, enc, sl, backward);

  if (
#ifdef TEST_PATTERN
      1 ||
#endif
      data_pattern_size != pattern_size) {
    data_pattern = (unsigned char *)malloc(data_pattern_size != 0
                                           ? data_pattern_size : 1);
    data_pattern_end = data_pattern + data_pattern_size;
    memcpy(data_pattern, data, data_pattern_size);
  }
  else {
    data_pattern     = pattern;
    data_pattern_end = pattern_end;
  }

  data += pattern_size;
  rem_size -= pattern_size;
  rem_size = fix_size(rem_size, enc, sl, backward);
#ifdef STANDALONE
  fprintf(stdout, "rem_size: %ld\n", rem_size);
#endif

  unsigned char *str = (unsigned char*)malloc(rem_size != 0 ? rem_size : 1);
  memcpy(str, data, rem_size);
  str_null_end = str + rem_size;

  r = exec(enc, options, syntax,
           (char* )pattern,      (char* )pattern_end,
           (char* )data_pattern, (char* )data_pattern_end,
           (char* )str, str_null_end, backward, sl);

  if (data_pattern != pattern)
    free(data_pattern);

  free(pattern);
  free(str);
  return r;
}

#ifdef PREV_CONTROL
#ifdef SYNTAX_TEST
#define NUM_CONTROL_BYTES      7
#else
#define NUM_CONTROL_BYTES      6
#endif
#else
#ifdef SYNTAX_TEST
#define NUM_CONTROL_BYTES      8
#else
#define NUM_CONTROL_BYTES      7
#endif
#endif

int LLVMFuzzerTestOneInput(const uint8_t * Data, size_t Size)
{
#if !defined(UTF16_BE) && !defined(UTF16_LE)
  static OnigEncoding encodings[] = {
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_ASCII,
    ONIG_ENCODING_EUC_JP,
    ONIG_ENCODING_EUC_TW,
    ONIG_ENCODING_EUC_KR,
    ONIG_ENCODING_EUC_CN,
    ONIG_ENCODING_SJIS,
    ONIG_ENCODING_KOI8_R,
    ONIG_ENCODING_CP1251,
    ONIG_ENCODING_BIG5,
    ONIG_ENCODING_GB18030,
    ONIG_ENCODING_UTF16_BE,
    ONIG_ENCODING_UTF16_LE,
    ONIG_ENCODING_UTF16_BE,
    ONIG_ENCODING_UTF16_LE,
    ONIG_ENCODING_UTF32_BE,
    ONIG_ENCODING_UTF32_LE,
    ONIG_ENCODING_UTF32_BE,
    ONIG_ENCODING_UTF32_LE,
    ONIG_ENCODING_ISO_8859_1,
    ONIG_ENCODING_ISO_8859_2,
    ONIG_ENCODING_ISO_8859_3,
    ONIG_ENCODING_ISO_8859_4,
    ONIG_ENCODING_ISO_8859_5,
    ONIG_ENCODING_ISO_8859_6,
    ONIG_ENCODING_ISO_8859_7,
    ONIG_ENCODING_ISO_8859_8,
    ONIG_ENCODING_ISO_8859_9,
    ONIG_ENCODING_ISO_8859_10,
    ONIG_ENCODING_ISO_8859_11,
    ONIG_ENCODING_ISO_8859_13,
    ONIG_ENCODING_ISO_8859_14,
    ONIG_ENCODING_ISO_8859_15,
    ONIG_ENCODING_ISO_8859_16
  };
  unsigned char encoding_choice;
#endif

#ifdef SYNTAX_TEST
  static OnigSyntaxType* syntaxes[] = {
    ONIG_SYNTAX_POSIX_EXTENDED,
    ONIG_SYNTAX_EMACS,
    ONIG_SYNTAX_GREP,
    ONIG_SYNTAX_GNU_REGEX,
    ONIG_SYNTAX_JAVA,
    ONIG_SYNTAX_PERL_NG,
    ONIG_SYNTAX_PYTHON,
    ONIG_SYNTAX_ONIGURUMA
  };

#ifdef STANDALONE
  static char* syntax_names[] = {
    "Posix Extended",
    "Emacs",
    "Grep",
    "GNU Regex",
    "Java",
    "Perl+NG",
    "Python",
    "Oniguruma"
  };
#endif

  unsigned char syntax_choice;
#endif

  int r;
  int backward;
  int pattern_size;
  size_t rem_size;
  unsigned char *data;
  unsigned char pattern_size_choice;
  OnigOptionType  options;
  OnigEncoding    enc;
  OnigSyntaxType* syntax;

#ifndef STANDALONE
#ifdef EXEC_PRINT_INTERVAL
  static FILE* STAT_FP;
#endif
#endif

  INPUT_COUNT++;

#ifdef DUMP_DATA_INTERVAL
  if (INPUT_COUNT % DUMP_DATA_INTERVAL == 0) {
    char path[20];
    sprintf(path, "dump-%ld", INPUT_COUNT);
    dump_file(path, (unsigned char* )Data, Size);
  }
#endif

  if (Size < NUM_CONTROL_BYTES) return 0;

  rem_size = Size;
  data = (unsigned char* )(Data);

#ifdef UTF16_BE
  enc = ONIG_ENCODING_UTF16_BE;
#else
#ifdef UTF16_LE
  enc = ONIG_ENCODING_UTF16_LE;
#else
  encoding_choice = data[0];
  data++;
  rem_size--;

  int num_encodings = sizeof(encodings)/sizeof(encodings[0]);
  enc = encodings[encoding_choice % num_encodings];
#endif
#endif

#ifdef SYNTAX_TEST
  syntax_choice = data[0];
  data++;
  rem_size--;

  int num_syntaxes = sizeof(syntaxes)/sizeof(syntaxes[0]);
  syntax = syntaxes[syntax_choice % num_syntaxes];
#else
  syntax = ONIG_SYNTAX_DEFAULT;
#endif

#ifdef PREV_CONTROL
  if ((data[2] & 0xc0) == 0)
    options = data[0] | (data[1] << 8) | (data[2] << 16);
#else
  if ((data[3] & 0xc0) == 0)
    options = data[0] | (data[1] << 8) | (data[2] << 16) | (data[3] << 24);
#endif
  else
    options = data[0] & ONIG_OPTION_IGNORECASE;

  data++; rem_size--;
  data++; rem_size--;
  data++; rem_size--;
#ifndef PREV_CONTROL
  data++; rem_size--;
#endif

  pattern_size_choice = data[0];
  data++; rem_size--;

  backward = (data[0] == 0xbb);
  data++; rem_size--;

  if (backward != 0) {
    options = options & ~ONIG_OPTION_FIND_LONGEST;
  }

  if (rem_size == 0)
    pattern_size = 0;
  else {
    pattern_size = (int )pattern_size_choice % rem_size;
    ADJUST_LEN(enc, pattern_size);
  }

#ifdef STANDALONE
  {
    char soptions[33];

    dump_data(stdout, data, pattern_size);
    to_binary(options, soptions);
#ifdef SYNTAX_TEST
    fprintf(stdout,
            "enc: %s, syntax: %s, pattern_size: %d, back:%d\noptions: %s\n",
            ONIGENC_NAME(enc),
            syntax_names[syntax_choice % num_syntaxes],
            pattern_size, backward, soptions);
#else
    fprintf(stdout, "enc: %s, pattern_size: %d, back:%d\noptions: %s\n",
            ONIGENC_NAME(enc), pattern_size, backward, soptions);
#endif

    print_options(stdout, options);
    fprintf(stdout, "\n");
  }
#endif

#ifdef DUMP_INPUT
  dump_input((unsigned char* )Data, Size);
#endif

  r = alloc_exec(enc, options, syntax, backward, pattern_size,
                 rem_size, data);
  if (r == -2) exit(-2);

#ifndef STANDALONE
#ifdef EXEC_PRINT_INTERVAL
  if (EXEC_COUNT_INTERVAL == EXEC_PRINT_INTERVAL) {
    float fexec, freg, fvalid;

    if (STAT_FP == 0) {
#ifdef STAT_PATH
      STAT_FP = fopen(STAT_PATH, "a");
#else
      STAT_FP = stdout;
#endif
    }

    output_current_time(STAT_FP);

    if (INPUT_COUNT != 0) { // overflow check
      fexec  = (float )EXEC_COUNT / INPUT_COUNT;
      freg   = (float )REGEX_SUCCESS_COUNT / INPUT_COUNT;
      fvalid = (float )VALID_STRING_COUNT / INPUT_COUNT;

      fprintf(STAT_FP, ": %ld: EXEC:%.2f, REG:%.2f, VALID:%.2f\n",
              EXEC_COUNT, fexec, freg, fvalid);
      fflush(STAT_FP);
    }
    else {
      fprintf(STAT_FP, ": ignore (input count overflow)\n");
    }

    EXEC_COUNT_INTERVAL = 0;
  }
  else if (EXEC_COUNT == 1) {
    output_current_time(stdout);
    fprintf(stdout, ": ------------ START ------------\n");
  }
#endif
#endif

  return r;
}

#ifdef STANDALONE

#define MAX_INPUT_DATA_SIZE  4194304

extern int main(int argc, char* argv[])
{
  size_t max_size;
  size_t n;
  uint8_t Data[MAX_INPUT_DATA_SIZE];

  if (argc > 1) {
    max_size = (size_t )atoi(argv[1]);
  }
  else {
    max_size = sizeof(Data);
  }

  n = read(0, Data, max_size);
  fprintf(stdout, "read size: %ld, max_size: %ld\n", n, max_size);

  LLVMFuzzerTestOneInput(Data, n);
  return 0;
}
#endif /* STANDALONE */
