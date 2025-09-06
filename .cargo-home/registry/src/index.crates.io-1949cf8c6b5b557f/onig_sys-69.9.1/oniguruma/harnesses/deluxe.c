/*
 * deluxe.c
 * contributed by Mark Griffin
 */
#include <stdio.h>
#include "oniguruma.h"

#include <stdlib.h>
#include <string.h>

#define RETRY_LIMIT   10000
#define DEPTH_LIMIT      10

typedef unsigned char uint8_t;

static int
search(regex_t* reg, unsigned char* str, unsigned char* end)
{
  int r;
  unsigned char *start, *range;
  OnigRegion *region;

  region = onig_region_new();

  start = str;
  range = end;
  r = onig_search(reg, str, end, start, range, region, ONIG_OPTION_NONE);
  if (r >= 0) {
    int i;

    fprintf(stdout, "match at %d  (%s)\n", r,
            ONIGENC_NAME(onig_get_encoding(reg)));
    for (i = 0; i < region->num_regs; i++) {
      fprintf(stdout, "%d: (%d-%d)\n", i, region->beg[i], region->end[i]);
    }
  }
  else if (r == ONIG_MISMATCH) {
    fprintf(stdout, "search fail (%s)\n",
            ONIGENC_NAME(onig_get_encoding(reg)));
  }
  else { /* error */
    char s[ONIG_MAX_ERROR_MESSAGE_LEN];
    onig_error_code_to_str((UChar* )s, r);
    fprintf(stdout, "ERROR: %s\n", s);
    fprintf(stdout, "  (%s)\n", ONIGENC_NAME(onig_get_encoding(reg)));
    onig_region_free(region, 1 /* 1:free self, 0:free contents only */);
    return -1;
  }

  onig_region_free(region, 1 /* 1:free self, 0:free contents only */);
  return 0;
}

static OnigCaseFoldType CF = ONIGENC_CASE_FOLD_MIN;

static int
exec_deluxe(OnigEncoding pattern_enc, OnigEncoding str_enc,
            OnigOptionType options, char* apattern, char* apattern_end,
            char* astr, char* astr_end)
{
  int r;
  regex_t* reg;
  OnigCompileInfo ci;
  OnigErrorInfo einfo;
  UChar* pattern = (UChar* )apattern;
  UChar* str     = (UChar* )astr;
  UChar* pattern_end = (UChar* )apattern_end;
  unsigned char* end = (unsigned char* )astr_end;

  onig_initialize(&str_enc, 1);
  onig_set_retry_limit_in_search(RETRY_LIMIT);
  onig_set_parse_depth_limit(DEPTH_LIMIT);

  ci.num_of_elements = 5;
  ci.pattern_enc = pattern_enc;
  ci.target_enc  = str_enc;
  ci.syntax      = ONIG_SYNTAX_DEFAULT;
  ci.option      = options;
  ci.case_fold_flag  = CF;

  r = onig_new_deluxe(&reg, pattern, pattern_end, &ci, &einfo);
  if (r != ONIG_NORMAL) {
    char s[ONIG_MAX_ERROR_MESSAGE_LEN];
    onig_error_code_to_str((UChar* )s, r, &einfo);
    fprintf(stdout, "ERROR: %s\n", s);
    onig_end();
    return -1;
  }

  if (onigenc_is_valid_mbc_string(str_enc, str, end) != 0) {
    r = search(reg, str, end);
  }

  onig_free(reg);
  onig_end();
  return 0;
}

#define PATTERN_SIZE 48 
#define NUM_CONTROL_BYTES 1
#define MIN_STR_SIZE  2
int LLVMFuzzerTestOneInput(const uint8_t * Data, size_t Size)
{
  int r;
  size_t remaining_size;
  unsigned char *data;
  unsigned char pat_encoding_choice;
  unsigned char str_encoding_choice;
  unsigned char *pattern;
  unsigned char *str;
  unsigned char *pattern_end;
  unsigned char *str_end;
  unsigned int num_encodings;
  OnigEncodingType *pattern_enc;
  OnigEncodingType *str_enc;

  OnigEncodingType *encodings[] = {
    ONIG_ENCODING_ASCII,
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
    ONIG_ENCODING_ISO_8859_16,
    ONIG_ENCODING_UTF8,
    ONIG_ENCODING_UTF16_BE,
    ONIG_ENCODING_UTF16_LE,
    ONIG_ENCODING_UTF32_BE,
    ONIG_ENCODING_UTF32_LE,
    ONIG_ENCODING_EUC_JP,
    ONIG_ENCODING_EUC_TW,
    ONIG_ENCODING_EUC_KR,
    ONIG_ENCODING_EUC_CN,
    ONIG_ENCODING_SJIS,
    //ONIG_ENCODING_KOI8,
    ONIG_ENCODING_KOI8_R,
    ONIG_ENCODING_CP1251,
    ONIG_ENCODING_BIG5,
    ONIG_ENCODING_GB18030,
  };

  if (Size <= (NUM_CONTROL_BYTES + PATTERN_SIZE + MIN_STR_SIZE))
    return 0;
  if (Size > 0x1000)
    return 0;

  remaining_size = Size;
  data = (unsigned char *)(Data);

  // pull off bytes to switch off
  pat_encoding_choice = data[0];
  data++;
  remaining_size--;
  str_encoding_choice = data[0];
  data++;
  remaining_size--;

  // copy first PATTERN_SIZE bytes off to be the pattern
  pattern = (unsigned char *)malloc(PATTERN_SIZE);
  memcpy(pattern, data, PATTERN_SIZE);
  pattern_end = pattern + PATTERN_SIZE;
  data += PATTERN_SIZE;
  remaining_size -= PATTERN_SIZE;

  str = (unsigned char*)malloc(remaining_size);
  memcpy(str, data, remaining_size);
  str_end = str + remaining_size;

  num_encodings = sizeof(encodings) / sizeof(encodings[0]);
  pattern_enc = encodings[pat_encoding_choice % num_encodings];
  str_enc = encodings[str_encoding_choice % num_encodings];

  r = exec_deluxe(pattern_enc, str_enc, ONIG_OPTION_NONE, (char *)pattern, (char *)pattern_end, (char *)str, (char *)str_end);

  free(pattern);
  free(str);

  return r;
}


#ifdef STANDALONE

#include <unistd.h>

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
