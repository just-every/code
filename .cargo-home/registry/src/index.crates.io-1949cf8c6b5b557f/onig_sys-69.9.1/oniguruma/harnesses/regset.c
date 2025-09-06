/*
 * regset.c
 * Copyright (c) 2019  K.Kosako
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


#define RETRY_LIMIT   5000

#ifdef STANDALONE
//#define CHECK_EACH_REGEX_SEARCH_TIME
#endif

#define MAX_REG_NUM   256

typedef unsigned char uint8_t;
static OnigEncoding ENC;

static void
output_current_time(FILE* fp)
{
  char d[64];
  time_t t;

  t = time(NULL);
  strftime(d, sizeof(d), "%m/%d %H:%M:%S", localtime(&t));

  fprintf(fp, "%s", d);
}

#ifdef CHECK_EACH_REGEX_SEARCH_TIME
static double
get_sec(struct timespec* ts, struct timespec* te)
{
  double t;

  t = (te->tv_sec - ts->tv_sec) +
      (double )(te->tv_nsec - ts->tv_nsec) / 1000000000.0;
  return t;
}

static int
check_each_regex_search_time(OnigRegSet* set, unsigned char* str, unsigned char* end)
{
  int n;
  int i;
  int r;
  OnigRegion* region;

  n = onig_regset_number_of_regex(set);
  region = onig_region_new();

  for (i = 0; i < n; i++) {
    regex_t* reg;
    unsigned char* start;
    unsigned char* range;
    struct timespec ts1, ts2;
    double t;

    reg = onig_regset_get_regex(set, i);
    start = str;
    range = end;

    clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &ts1);

    r = onig_search(reg, str, end, start, range, region, ONIG_OPTION_NONE);

    clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &ts2);
    t = get_sec(&ts1, &ts2);

    fprintf(stdout, "regex search time %d: %6.2lfmsec.\n", i, t * 1000.0);
  }

  onig_region_free(region, 1);
  return 0;
}
#endif

static int
search(OnigRegSet* set, OnigRegSetLead lead, unsigned char* str, unsigned char* end)
{
  int r;
  int match_pos;
  unsigned char *start, *range;

  start = str;
  range = end;
  r = onig_regset_search(set, str, end, start, range, lead,
                         ONIG_OPTION_NONE, &match_pos);
  if (r >= 0) {
#ifdef STANDALONE
    int i;
    int match_index;
    OnigRegion* region;

    match_index = r;
    fprintf(stdout, "match reg index: %d, pos: %d  (%s)\n",
            match_index, match_pos, ONIGENC_NAME(ENC));
    region = onig_regset_get_region(set, match_index);
    if (region == 0) {
      fprintf(stdout, "ERROR: can't get region.\n");
      return -1;
    }

    for (i = 0; i < region->num_regs; i++) {
      fprintf(stdout, "%d: (%d-%d)\n", i, region->beg[i], region->end[i]);
    }
#endif
  }
  else if (r == ONIG_MISMATCH) {
#ifdef STANDALONE
    fprintf(stdout, "search fail (%s)\n", ONIGENC_NAME(ENC));
#endif
  }
  else { /* error */
#ifdef STANDALONE
    char s[ONIG_MAX_ERROR_MESSAGE_LEN];

    onig_error_code_to_str((UChar* )s, r);
    fprintf(stdout, "ERROR: %s\n", s);
    fprintf(stdout, "  (%s)\n", ONIGENC_NAME(ENC));
#endif
    return -1;
  }

  return 0;
}

static long INPUT_COUNT;
static long EXEC_COUNT;
static long EXEC_COUNT_INTERVAL;
static long REGEX_SUCCESS_COUNT;
static long VALID_STRING_COUNT;

static int
exec(OnigEncoding enc, int reg_num, int init_reg_num,
     UChar* pat[], UChar* pat_end[],
     OnigRegSetLead lead, UChar* str, UChar* end)
{
  int r;
  int i, j;
  OnigRegSet* set;
  regex_t* reg;
  OnigOptionType options;
  OnigErrorInfo einfo;
  regex_t* regs[MAX_REG_NUM];

  EXEC_COUNT++;
  EXEC_COUNT_INTERVAL++;

  options = (EXEC_COUNT % 4 == 0) ? ONIG_OPTION_IGNORECASE : ONIG_OPTION_NONE;

  onig_initialize(&enc, 1);
  onig_set_retry_limit_in_search(RETRY_LIMIT);

  for (i = 0; i < init_reg_num; i++) {
    r = onig_new(&regs[i], pat[i], pat_end[i], options, ENC,
                 ONIG_SYNTAX_DEFAULT, &einfo);
    if (r != 0) {
#ifdef STANDALONE
      char s[ONIG_MAX_ERROR_MESSAGE_LEN];

      onig_error_code_to_str((UChar* )s, r, &einfo);
      fprintf(stdout, "ERROR: index: %d, %s\n", i, s);
#endif

      for (j = 0; j < i; j++) onig_free(regs[j]);

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
  }

  r = onig_regset_new(&set, init_reg_num, regs);
  if (r != 0) {
    for (i = 0; i < init_reg_num; i++) {
      onig_free(regs[i]);
    }
    onig_end();
    return -1;
  }

  for (i = init_reg_num; i < reg_num; i++) {
    r = onig_new(&reg, pat[i], pat_end[i], options, ENC,
                 ONIG_SYNTAX_DEFAULT, &einfo);
    if (r != 0) {
#ifdef STANDALONE
      char s[ONIG_MAX_ERROR_MESSAGE_LEN];

      onig_error_code_to_str((UChar* )s, r, &einfo);
      fprintf(stdout, "ERROR: index: %d, %s\n", i, s);
#endif
      onig_regset_free(set);
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

    r = onig_regset_add(set, reg);
    if (r != 0) {
      onig_regset_free(set);
      onig_end();
      fprintf(stdout, "ERROR: onig_regset_add(): %d\n", i);
      return r;
    }
  }

  REGEX_SUCCESS_COUNT++;

  if (onigenc_is_valid_mbc_string(enc, str, end) != 0) {
    VALID_STRING_COUNT++;
    r = search(set, lead, str, end);
#ifdef CHECK_EACH_REGEX_SEARCH_TIME
    r = check_each_regex_search_time(set, str, end);
#endif
  }

  onig_regset_free(set);
  onig_end();
  return 0;
}

#define MAX_PATTERN_SIZE      30
#define NUM_CONTROL_BYTES      3

#define EXEC_PRINT_INTERVAL  2000000

static int MaxRegNum;
static int MaxInitRegNum;

extern int
LLVMFuzzerTestOneInput(const uint8_t * Data, size_t Size)
{
  int r, i;
  int pattern_size;
  unsigned char *str_null_end;
  size_t remaining_size;
  unsigned char *data;
  unsigned int reg_num;
  unsigned int init_reg_num;
  unsigned char* pat[256];
  unsigned char* pat_end[256];
  int len;
  unsigned int lead_num;
  OnigRegSetLead lead;

  INPUT_COUNT++;

  if (Size < NUM_CONTROL_BYTES) return 0;

  remaining_size = Size;
  data = (unsigned char* )(Data);

  reg_num = data[0];
  data++;
  remaining_size--;

  init_reg_num = data[0];
  data++;
  remaining_size--;

  lead_num = data[0];
  data++;
  remaining_size--;
  lead = (lead_num % 2 == 0 ? ONIG_REGSET_POSITION_LEAD : ONIG_REGSET_REGEX_LEAD);

  if (remaining_size < reg_num * 2) {
    reg_num = reg_num % 15;  // zero is OK.
  }

  init_reg_num %= (reg_num + 1);

  if (MaxRegNum < reg_num)
    MaxRegNum = reg_num;

  if (MaxInitRegNum < init_reg_num)
    MaxInitRegNum = init_reg_num;

  if (reg_num == 0)
    pattern_size = 1;
  else
    pattern_size = remaining_size / (reg_num * 2);
    
  if (pattern_size > MAX_PATTERN_SIZE)
    pattern_size = MAX_PATTERN_SIZE;

  len = pattern_size * reg_num;
  if (len == 0) len = 1;

  for (i = 0; i < reg_num; i++) {
    pat[i] = (unsigned char* )malloc(pattern_size);
    memcpy(pat[i], data, pattern_size);
    pat_end[i] = pat[i] + pattern_size;
    data += pattern_size;
    remaining_size -= pattern_size;
  }

  unsigned char *str = (unsigned char*)malloc(remaining_size != 0 ? remaining_size : 1);
  memcpy(str, data, remaining_size);
  str_null_end = str + remaining_size;

#ifdef STANDALONE
  fprintf(stdout, "reg num: %d, pattern size: %d, lead: %s\n",
          reg_num, pattern_size,
          lead == ONIG_REGSET_POSITION_LEAD ? "position" : "regex");

  if (reg_num != 0) {
    unsigned char* p;
    i = 0;
    p = pat[0];
    while (p < pat_end[0]) {
      fprintf(stdout, " 0x%02x", (int )*p++);
      i++;
      if (i % 8 == 0) fprintf(stdout, "\n");
    }
    fprintf(stdout, "\n");
  }
#endif

  ENC = ONIG_ENCODING_UTF8;

  r = exec(ENC, reg_num, init_reg_num, pat, pat_end, lead, str, str_null_end);

  for (i = 0; i < reg_num; i++) {
    free(pat[i]);
  }
  free(str);

  if (r == -2) {
    //output_data("parser-bug", Data, Size);
    exit(-2);
  }

  if (EXEC_COUNT_INTERVAL == EXEC_PRINT_INTERVAL) {
    float fexec, freg, fvalid;

    fexec  = (float )EXEC_COUNT / INPUT_COUNT;
    freg   = (float )REGEX_SUCCESS_COUNT / INPUT_COUNT;
    fvalid = (float )VALID_STRING_COUNT / INPUT_COUNT;

    output_current_time(stdout);
    fprintf(stdout, ": %ld: EXEC:%.2f, REG:%.2f, VALID:%.2f MAX REG:%d-%d\n",
            EXEC_COUNT, fexec, freg, fvalid, MaxRegNum, MaxInitRegNum);

    EXEC_COUNT_INTERVAL = 0;
  }
  else if (EXEC_COUNT == 1) {
    output_current_time(stdout);
    fprintf(stdout, ": ------------ START ------------\n");
  }

  return r;
}

#ifdef STANDALONE

extern int main(int argc, char* argv[])
{
  size_t n;
  uint8_t Data[10000];

  n = read(0, Data, sizeof(Data));
  fprintf(stdout, "n: %ld\n", n);
  LLVMFuzzerTestOneInput(Data, n);

  return 0;
}
#endif /* STANDALONE */
