/* Header-only translation unit: including the generated enum projection
   header alone must compile freestanding with warnings as errors and no
   projection call. Registered in CMakeLists.txt as an object library with
   the same strict options used by the manual compiler commands in the task
   brief (gcc/clang -std=c99 -ffreestanding -Wall -Wextra -Werror
   -fsyntax-only); this file exists so the proof also runs under CTest. */

#include "enums/cmis_data_path_state.h"

/* Intentionally no main, no variables and no calls: unused-header hygiene is
   the whole point of this unit. */
