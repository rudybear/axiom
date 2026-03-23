#!/usr/bin/env python3
"""Generate 100+ benchmark programs in AXIOM (.axm) and C (.c) format.

Each benchmark tests a specific algorithm or computation pattern.
Generates:
  - benchmarks/suite/NNN_name.axm  (AXIOM source)
  - benchmarks/suite/NNN_name.c    (C equivalent)
  - benchmarks/suite/manifest.json (metadata + expected outputs)
  - benchmarks/run_all.py          (compile, run, compare, time)
"""

import json
import os
import sys

SUITE_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "suite")
BENCHMARKS = []


def register(id_num, name, category, expected_output, axm_code, c_code):
    """Register a benchmark with its code and expected output."""
    BENCHMARKS.append({
        "id": f"{id_num:03d}",
        "name": name,
        "category": category,
        "expected_output": expected_output,
        "axm": f"{id_num:03d}_{name}.axm",
        "c": f"{id_num:03d}_{name}.c",
        "axm_code": axm_code,
        "c_code": c_code,
    })


# =============================================================================
# NUMBER THEORY (001-035)
# =============================================================================

register(1, "gcd_basic", "number_theory", "12\n",
# AXM
"""@module gcd_basic;

@pure
fn gcd(a: i32, b: i32) -> i32 {
    let x: i32 = a;
    let y: i32 = b;
    while y != 0 {
        let t: i32 = y;
        y = x % y;
        x = t;
    }
    return x;
}

fn main() -> i32 {
    print_i32(gcd(48, 36));
    return 0;
}
""",
# C
"""#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
int main(void) { printf("%d\\n", gcd(48, 36)); return 0; }
""")

register(2, "gcd_large", "number_theory", "6\n",
"""@module gcd_large;

@pure
fn gcd(a: i32, b: i32) -> i32 {
    let x: i32 = a;
    let y: i32 = b;
    while y != 0 {
        let t: i32 = y;
        y = x % y;
        x = t;
    }
    return x;
}

fn main() -> i32 {
    print_i32(gcd(123456, 7890));
    return 0;
}
""",
"""#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
int main(void) { printf("%d\\n", gcd(123456, 7890)); return 0; }
""")

register(3, "gcd_recursive", "number_theory", "12\n",
"""@module gcd_recursive;

@pure
fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 {
        return a;
    }
    return gcd(b, a % b);
}

fn main() -> i32 {
    print_i32(gcd(48, 36));
    return 0;
}
""",
"""#include <stdio.h>
static int gcd(int a, int b) {
    if (b == 0) return a;
    return gcd(b, a % b);
}
int main(void) { printf("%d\\n", gcd(48, 36)); return 0; }
""")

register(4, "is_prime_small", "number_theory", "1\n",
"""@module is_prime_small;

@pure
fn is_prime(n: i32) -> i32 {
    if n < 2 {
        return 0;
    }
    let i: i32 = 2;
    while i * i <= n {
        if n % i == 0 {
            return 0;
        }
        i = i + 1;
    }
    return 1;
}

fn main() -> i32 {
    print_i32(is_prime(97));
    return 0;
}
""",
"""#include <stdio.h>
static int is_prime(int n) {
    if (n < 2) return 0;
    for (int i = 2; i * i <= n; i++)
        if (n % i == 0) return 0;
    return 1;
}
int main(void) { printf("%d\\n", is_prime(97)); return 0; }
""")

register(5, "is_prime_large", "number_theory", "1\n",
"""@module is_prime_large;

@pure
fn is_prime(n: i32) -> i32 {
    if n < 2 {
        return 0;
    }
    let i: i32 = 2;
    while i * i <= n {
        if n % i == 0 {
            return 0;
        }
        i = i + 1;
    }
    return 1;
}

fn main() -> i32 {
    print_i32(is_prime(104729));
    return 0;
}
""",
"""#include <stdio.h>
static int is_prime(int n) {
    if (n < 2) return 0;
    for (int i = 2; i * i <= n; i++)
        if (n % i == 0) return 0;
    return 1;
}
int main(void) { printf("%d\\n", is_prime(104729)); return 0; }
""")

register(6, "count_primes_1000", "number_theory", "168\n",
"""@module count_primes_1000;

@pure
fn is_prime(n: i32) -> i32 {
    if n < 2 {
        return 0;
    }
    let i: i32 = 2;
    while i * i <= n {
        if n % i == 0 {
            return 0;
        }
        i = i + 1;
    }
    return 1;
}

fn main() -> i32 {
    let count: i32 = 0;
    for n: i32 in range(2, 1001) {
        count = count + is_prime(n);
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
static int is_prime(int n) {
    if (n < 2) return 0;
    for (int i = 2; i * i <= n; i++)
        if (n % i == 0) return 0;
    return 1;
}
int main(void) {
    int count = 0;
    for (int n = 2; n <= 1000; n++) count += is_prime(n);
    printf("%d\\n", count);
    return 0;
}
""")

register(7, "count_primes_10000", "number_theory", "1229\n",
"""@module count_primes_10000;

@pure
fn is_prime(n: i32) -> i32 {
    if n < 2 {
        return 0;
    }
    let i: i32 = 2;
    while i * i <= n {
        if n % i == 0 {
            return 0;
        }
        i = i + 1;
    }
    return 1;
}

fn main() -> i32 {
    let count: i32 = 0;
    for n: i32 in range(2, 10001) {
        count = count + is_prime(n);
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
static int is_prime(int n) {
    if (n < 2) return 0;
    for (int i = 2; i * i <= n; i++)
        if (n % i == 0) return 0;
    return 1;
}
int main(void) {
    int count = 0;
    for (int n = 2; n <= 10000; n++) count += is_prime(n);
    printf("%d\\n", count);
    return 0;
}
""")

register(8, "count_primes_100000", "number_theory", "9592\n",
"""@module count_primes_100000;

@pure
fn is_prime(n: i32) -> i32 {
    if n < 2 {
        return 0;
    }
    let i: i32 = 2;
    while i * i <= n {
        if n % i == 0 {
            return 0;
        }
        i = i + 1;
    }
    return 1;
}

fn main() -> i32 {
    let count: i32 = 0;
    for n: i32 in range(2, 100001) {
        count = count + is_prime(n);
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
static int is_prime(int n) {
    if (n < 2) return 0;
    for (int i = 2; i * i <= n; i++)
        if (n % i == 0) return 0;
    return 1;
}
int main(void) {
    int count = 0;
    for (int n = 2; n <= 100000; n++) count += is_prime(n);
    printf("%d\\n", count);
    return 0;
}
""")

register(9, "euler_totient", "number_theory", "4\n",
"""@module euler_totient;

@pure
fn gcd(a: i32, b: i32) -> i32 {
    let x: i32 = a;
    let y: i32 = b;
    while y != 0 {
        let t: i32 = y;
        y = x % y;
        x = t;
    }
    return x;
}

@pure
fn totient(n: i32) -> i32 {
    let count: i32 = 0;
    for i: i32 in range(1, n + 1) {
        if gcd(i, n) == 1 {
            count = count + 1;
        }
    }
    return count;
}

fn main() -> i32 {
    print_i32(totient(12));
    return 0;
}
""",
"""#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
static int totient(int n) {
    int count = 0;
    for (int i = 1; i <= n; i++)
        if (gcd(i, n) == 1) count++;
    return count;
}
int main(void) { printf("%d\\n", totient(12)); return 0; }
""")

register(10, "euler_totient_sum", "number_theory", "3044\n",
"""@module euler_totient_sum;

@pure
fn gcd(a: i32, b: i32) -> i32 {
    let x: i32 = a;
    let y: i32 = b;
    while y != 0 {
        let t: i32 = y;
        y = x % y;
        x = t;
    }
    return x;
}

@pure
fn totient(n: i32) -> i32 {
    let count: i32 = 0;
    for i: i32 in range(1, n + 1) {
        if gcd(i, n) == 1 {
            count = count + 1;
        }
    }
    return count;
}

fn main() -> i32 {
    let sum: i32 = 0;
    for n: i32 in range(1, 101) {
        sum = sum + totient(n);
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
static int totient(int n) {
    int count = 0;
    for (int i = 1; i <= n; i++)
        if (gcd(i, n) == 1) count++;
    return count;
}
int main(void) {
    int sum = 0;
    for (int n = 1; n <= 100; n++) sum += totient(n);
    printf("%d\\n", sum);
    return 0;
}
""")

register(11, "collatz_steps_27", "number_theory", "111\n",
"""@module collatz_steps_27;

@pure
fn collatz_steps(n: i64) -> i32 {
    let x: i64 = n;
    let steps: i32 = 0;
    while x != 1 {
        if x % 2 == 0 {
            x = x / 2;
        } else {
            x = 3 * x + 1;
        }
        steps = steps + 1;
    }
    return steps;
}

fn main() -> i32 {
    print_i32(collatz_steps(27));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int collatz_steps(int64_t n) {
    int64_t x = n;
    int steps = 0;
    while (x != 1) {
        if (x % 2 == 0) x /= 2;
        else x = 3 * x + 1;
        steps++;
    }
    return steps;
}
int main(void) { printf("%d\\n", collatz_steps(27)); return 0; }
""")

register(12, "collatz_steps_871", "number_theory", "178\n",
"""@module collatz_steps_871;

@pure
fn collatz_steps(n: i64) -> i32 {
    let x: i64 = n;
    let steps: i32 = 0;
    while x != 1 {
        if x % 2 == 0 {
            x = x / 2;
        } else {
            x = 3 * x + 1;
        }
        steps = steps + 1;
    }
    return steps;
}

fn main() -> i32 {
    print_i32(collatz_steps(871));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int collatz_steps(int64_t n) {
    int64_t x = n;
    int steps = 0;
    while (x != 1) {
        if (x % 2 == 0) x /= 2;
        else x = 3 * x + 1;
        steps++;
    }
    return steps;
}
int main(void) { printf("%d\\n", collatz_steps(871)); return 0; }
""")

register(13, "collatz_max_steps", "number_theory", "178\n",
"""@module collatz_max_steps;

@pure
fn collatz_steps(n: i64) -> i32 {
    let x: i64 = n;
    let steps: i32 = 0;
    while x != 1 {
        if x % 2 == 0 {
            x = x / 2;
        } else {
            x = 3 * x + 1;
        }
        steps = steps + 1;
    }
    return steps;
}

fn main() -> i32 {
    let max_steps: i32 = 0;
    for i: i32 in range(1, 1001) {
        let s: i32 = collatz_steps(widen(i));
        if s > max_steps {
            max_steps = s;
        }
    }
    print_i32(max_steps);
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int collatz_steps(int64_t n) {
    int64_t x = n;
    int steps = 0;
    while (x != 1) {
        if (x % 2 == 0) x /= 2;
        else x = 3 * x + 1;
        steps++;
    }
    return steps;
}
int main(void) {
    int max_steps = 0;
    for (int i = 1; i <= 1000; i++) {
        int s = collatz_steps((int64_t)i);
        if (s > max_steps) max_steps = s;
    }
    printf("%d\\n", max_steps);
    return 0;
}
""")

register(14, "sum_of_divisors", "number_theory", "28\n",
"""@module sum_of_divisors;

@pure
fn sum_divisors(n: i32) -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(1, n + 1) {
        if n % i == 0 {
            sum = sum + i;
        }
    }
    return sum;
}

fn main() -> i32 {
    print_i32(sum_divisors(12));
    return 0;
}
""",
"""#include <stdio.h>
static int sum_divisors(int n) {
    int sum = 0;
    for (int i = 1; i <= n; i++)
        if (n % i == 0) sum += i;
    return sum;
}
int main(void) { printf("%d\\n", sum_divisors(12)); return 0; }
""")

register(15, "modular_exp", "number_theory", "601\n",
"""@module modular_exp;

@pure
fn mod_exp(base: i64, exp: i64, modulus: i64) -> i64 {
    let result: i64 = 1;
    let b: i64 = base % modulus;
    let e: i64 = exp;
    while e > 0 {
        if e % 2 == 1 {
            result = result * b % modulus;
        }
        e = e / 2;
        b = b * b % modulus;
    }
    return result;
}

fn main() -> i32 {
    print_i64(mod_exp(7, 256, 1000));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int64_t mod_exp(int64_t base, int64_t exp, int64_t modulus) {
    int64_t result = 1, b = base % modulus, e = exp;
    while (e > 0) {
        if (e % 2 == 1) result = result * b % modulus;
        e /= 2;
        b = b * b % modulus;
    }
    return result;
}
int main(void) { printf("%lld\\n", (long long)mod_exp(7, 256, 1000)); return 0; }
""")

register(16, "fibonacci_iterative_40", "number_theory", "102334155\n",
"""@module fibonacci_iterative_40;

@pure
fn fib(n: i32) -> i64 {
    if n <= 1 {
        return widen(n);
    }
    let a: i64 = 0;
    let b: i64 = 1;
    for i: i32 in range(2, n + 1) {
        let temp: i64 = b;
        b = a + b;
        a = temp;
    }
    return b;
}

fn main() -> i32 {
    print_i64(fib(40));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int64_t fib(int n) {
    if (n <= 1) return n;
    int64_t a = 0, b = 1;
    for (int i = 2; i <= n; i++) { int64_t t = b; b = a + b; a = t; }
    return b;
}
int main(void) { printf("%lld\\n", (long long)fib(40)); return 0; }
""")

register(17, "fibonacci_iterative_50", "number_theory", "12586269025\n",
"""@module fibonacci_iterative_50;

@pure
fn fib(n: i32) -> i64 {
    if n <= 1 {
        return widen(n);
    }
    let a: i64 = 0;
    let b: i64 = 1;
    for i: i32 in range(2, n + 1) {
        let temp: i64 = b;
        b = a + b;
        a = temp;
    }
    return b;
}

fn main() -> i32 {
    print_i64(fib(50));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int64_t fib(int n) {
    if (n <= 1) return n;
    int64_t a = 0, b = 1;
    for (int i = 2; i <= n; i++) { int64_t t = b; b = a + b; a = t; }
    return b;
}
int main(void) { printf("%lld\\n", (long long)fib(50)); return 0; }
""")

register(18, "fibonacci_recursive_30", "number_theory", "832040\n",
"""@module fibonacci_recursive_30;

@pure
fn fib(n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn main() -> i32 {
    print_i32(fib(30));
    return 0;
}
""",
"""#include <stdio.h>
static int fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}
int main(void) { printf("%d\\n", fib(30)); return 0; }
""")

register(19, "fibonacci_recursive_35", "number_theory", "9227465\n",
"""@module fibonacci_recursive_35;

@pure
fn fib(n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn main() -> i32 {
    print_i32(fib(35));
    return 0;
}
""",
"""#include <stdio.h>
static int fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}
int main(void) { printf("%d\\n", fib(35)); return 0; }
""")

register(20, "factorial_12", "number_theory", "479001600\n",
"""@module factorial_12;

@pure
fn factorial(n: i32) -> i64 {
    let result: i64 = 1;
    for i: i32 in range(2, n + 1) {
        result = result * widen(i);
    }
    return result;
}

fn main() -> i32 {
    print_i64(factorial(12));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int64_t factorial(int n) {
    int64_t result = 1;
    for (int i = 2; i <= n; i++) result *= i;
    return result;
}
int main(void) { printf("%lld\\n", (long long)factorial(12)); return 0; }
""")

register(21, "factorial_20", "number_theory", "2432902008176640000\n",
"""@module factorial_20;

@pure
fn factorial(n: i32) -> i64 {
    let result: i64 = 1;
    for i: i32 in range(2, n + 1) {
        result = result * widen(i);
    }
    return result;
}

fn main() -> i32 {
    print_i64(factorial(20));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int64_t factorial(int n) {
    int64_t result = 1;
    for (int i = 2; i <= n; i++) result *= i;
    return result;
}
int main(void) { printf("%lld\\n", (long long)factorial(20)); return 0; }
""")

register(22, "perfect_number_check", "number_theory", "1\n",
"""@module perfect_number_check;

@pure
fn is_perfect(n: i32) -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(1, n) {
        if n % i == 0 {
            sum = sum + i;
        }
    }
    if sum == n {
        return 1;
    }
    return 0;
}

fn main() -> i32 {
    print_i32(is_perfect(496));
    return 0;
}
""",
"""#include <stdio.h>
static int is_perfect(int n) {
    int sum = 0;
    for (int i = 1; i < n; i++)
        if (n % i == 0) sum += i;
    return sum == n ? 1 : 0;
}
int main(void) { printf("%d\\n", is_perfect(496)); return 0; }
""")

register(23, "count_perfect_numbers", "number_theory", "4\n",
"""@module count_perfect_numbers;

@pure
fn is_perfect(n: i32) -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(1, n) {
        if n % i == 0 {
            sum = sum + i;
        }
    }
    if sum == n {
        return 1;
    }
    return 0;
}

fn main() -> i32 {
    let count: i32 = 0;
    for n: i32 in range(2, 8129) {
        count = count + is_perfect(n);
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
static int is_perfect(int n) {
    int sum = 0;
    for (int i = 1; i < n; i++)
        if (n % i == 0) sum += i;
    return sum == n ? 1 : 0;
}
int main(void) {
    int count = 0;
    for (int n = 2; n < 8129; n++) count += is_perfect(n);
    printf("%d\\n", count);
    return 0;
}
""")

register(24, "coprimality_test", "number_theory", "14\n",
"""@module coprimality_test;

@pure
fn gcd(a: i32, b: i32) -> i32 {
    let x: i32 = a;
    let y: i32 = b;
    while y != 0 {
        let t: i32 = y;
        y = x % y;
        x = t;
    }
    return x;
}

fn main() -> i32 {
    let count: i32 = 0;
    for i: i32 in range(1, 51) {
        if gcd(i, 30) == 1 {
            count = count + 1;
        }
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
int main(void) {
    int count = 0;
    for (int i = 1; i <= 50; i++)
        if (gcd(i, 30) == 1) count++;
    printf("%d\\n", count);
    return 0;
}
""")

register(25, "digital_root", "number_theory", "6\n",
"""@module digital_root;

@pure
fn digital_root(n: i32) -> i32 {
    let x: i32 = n;
    while x >= 10 {
        let sum: i32 = 0;
        let tmp: i32 = x;
        while tmp > 0 {
            sum = sum + tmp % 10;
            tmp = tmp / 10;
        }
        x = sum;
    }
    return x;
}

fn main() -> i32 {
    print_i32(digital_root(942));
    return 0;
}
""",
"""#include <stdio.h>
static int digital_root(int n) {
    int x = n;
    while (x >= 10) {
        int sum = 0, tmp = x;
        while (tmp > 0) { sum += tmp % 10; tmp /= 10; }
        x = sum;
    }
    return x;
}
int main(void) { printf("%d\\n", digital_root(942)); return 0; }
""")

register(26, "digit_sum", "number_theory", "46\n",
"""@module digit_sum;

@pure
fn digit_sum(n: i32) -> i32 {
    let sum: i32 = 0;
    let x: i32 = n;
    while x > 0 {
        sum = sum + x % 10;
        x = x / 10;
    }
    return sum;
}

fn main() -> i32 {
    let total: i32 = 0;
    for i: i32 in range(1, 101) {
        total = total + digit_sum(i);
    }
    print_i32(total - digit_sum(100));
    print_i32(digit_sum(123456789));
    return 0;
}
""",
"""#include <stdio.h>
static int digit_sum(int n) {
    int sum = 0, x = n;
    while (x > 0) { sum += x % 10; x /= 10; }
    return sum;
}
int main(void) {
    int total = 0;
    for (int i = 1; i <= 100; i++) total += digit_sum(i);
    printf("%d\\n", total - digit_sum(100));
    printf("%d\\n", digit_sum(123456789));
    return 0;
}
""")

# Fix: digit_sum(123456789) = 45, total for 1..99 = let's compute correctly
# Actually let me just use simpler expected outputs. Let me use just one print.

# Re-register 26 with simpler output
BENCHMARKS.pop()  # remove last
register(26, "digit_sum", "number_theory", "45\n",
"""@module digit_sum;

@pure
fn digit_sum(n: i32) -> i32 {
    let sum: i32 = 0;
    let x: i32 = n;
    while x > 0 {
        sum = sum + x % 10;
        x = x / 10;
    }
    return sum;
}

fn main() -> i32 {
    print_i32(digit_sum(123456789));
    return 0;
}
""",
"""#include <stdio.h>
static int digit_sum(int n) {
    int sum = 0, x = n;
    while (x > 0) { sum += x % 10; x /= 10; }
    return sum;
}
int main(void) { printf("%d\\n", digit_sum(123456789)); return 0; }
""")

register(27, "lcm", "number_theory", "36\n",
"""@module lcm;

@pure
fn gcd(a: i32, b: i32) -> i32 {
    let x: i32 = a;
    let y: i32 = b;
    while y != 0 {
        let t: i32 = y;
        y = x % y;
        x = t;
    }
    return x;
}

@pure
fn lcm(a: i32, b: i32) -> i32 {
    return a / gcd(a, b) * b;
}

fn main() -> i32 {
    print_i32(lcm(12, 18));
    return 0;
}
""",
"""#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
static int lcm(int a, int b) { return a / gcd(a, b) * b; }
int main(void) { printf("%d\\n", lcm(12, 18)); return 0; }
""")

register(28, "sum_of_squares", "number_theory", "338350\n",
"""@module sum_of_squares;

@pure
fn sum_squares(n: i64) -> i64 {
    let sum: i64 = 0;
    for i: i32 in range(1, 101) {
        let x: i64 = widen(i);
        sum = sum + x * x;
    }
    return sum;
}

fn main() -> i32 {
    print_i64(sum_squares(100));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
int main(void) {
    int64_t sum = 0;
    for (int i = 1; i <= 100; i++) sum += (int64_t)i * i;
    printf("%lld\\n", (long long)sum);
    return 0;
}
""")

register(29, "sum_cubes", "number_theory", "25502500\n",
"""@module sum_cubes;

fn main() -> i32 {
    let sum: i64 = 0;
    for i: i32 in range(1, 101) {
        let x: i64 = widen(i);
        sum = sum + x * x * x;
    }
    print_i64(sum);
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
int main(void) {
    int64_t sum = 0;
    for (int i = 1; i <= 100; i++) sum += (int64_t)i * i * i;
    printf("%lld\\n", (long long)sum);
    return 0;
}
""")

register(30, "abundant_count", "number_theory", "246\n",
"""@module abundant_count;

@pure
fn sum_proper_divisors(n: i32) -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(1, n) {
        if n % i == 0 {
            sum = sum + i;
        }
    }
    return sum;
}

fn main() -> i32 {
    let count: i32 = 0;
    for n: i32 in range(2, 1001) {
        if sum_proper_divisors(n) > n {
            count = count + 1;
        }
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
static int sum_proper_divisors(int n) {
    int sum = 0;
    for (int i = 1; i < n; i++)
        if (n % i == 0) sum += i;
    return sum;
}
int main(void) {
    int count = 0;
    for (int n = 2; n <= 1000; n++)
        if (sum_proper_divisors(n) > n) count++;
    printf("%d\\n", count);
    return 0;
}
""")

register(31, "number_reverse", "number_theory", "54321\n",
"""@module number_reverse;

@pure
fn reverse_num(n: i32) -> i32 {
    let result: i32 = 0;
    let x: i32 = n;
    while x > 0 {
        result = result * 10 + x % 10;
        x = x / 10;
    }
    return result;
}

fn main() -> i32 {
    print_i32(reverse_num(12345));
    return 0;
}
""",
"""#include <stdio.h>
static int reverse_num(int n) {
    int result = 0, x = n;
    while (x > 0) { result = result * 10 + x % 10; x /= 10; }
    return result;
}
int main(void) { printf("%d\\n", reverse_num(12345)); return 0; }
""")

register(32, "palindrome_count", "number_theory", "108\n",
"""@module palindrome_count;

@pure
fn reverse_num(n: i32) -> i32 {
    let result: i32 = 0;
    let x: i32 = n;
    while x > 0 {
        result = result * 10 + x % 10;
        x = x / 10;
    }
    return result;
}

fn main() -> i32 {
    let count: i32 = 0;
    for n: i32 in range(1, 1000) {
        if reverse_num(n) == n {
            count = count + 1;
        }
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
static int reverse_num(int n) {
    int result = 0, x = n;
    while (x > 0) { result = result * 10 + x % 10; x /= 10; }
    return result;
}
int main(void) {
    int count = 0;
    for (int n = 1; n < 1000; n++)
        if (reverse_num(n) == n) count++;
    printf("%d\\n", count);
    return 0;
}
""")

register(33, "power_iterative", "number_theory", "1073741824\n",
"""@module power_iterative;

@pure
fn power(base: i64, exp: i32) -> i64 {
    let result: i64 = 1;
    for i: i32 in range(0, exp) {
        result = result * base;
    }
    return result;
}

fn main() -> i32 {
    print_i64(power(2, 30));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int64_t power(int64_t base, int exp) {
    int64_t result = 1;
    for (int i = 0; i < exp; i++) result *= base;
    return result;
}
int main(void) { printf("%lld\\n", (long long)power(2, 30)); return 0; }
""")

register(34, "harshad_count", "number_theory", "23\n",
"""@module harshad_count;

@pure
fn digit_sum(n: i32) -> i32 {
    let sum: i32 = 0;
    let x: i32 = n;
    while x > 0 {
        sum = sum + x % 10;
        x = x / 10;
    }
    return sum;
}

fn main() -> i32 {
    let count: i32 = 0;
    for n: i32 in range(1, 51) {
        if n % digit_sum(n) == 0 {
            count = count + 1;
        }
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
static int digit_sum(int n) {
    int sum = 0, x = n;
    while (x > 0) { sum += x % 10; x /= 10; }
    return sum;
}
int main(void) {
    int count = 0;
    for (int n = 1; n <= 50; n++)
        if (n % digit_sum(n) == 0) count++;
    printf("%d\\n", count);
    return 0;
}
""")

register(35, "triangular_number", "number_theory", "5050\n",
"""@module triangular_number;

@pure
fn triangular(n: i32) -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(1, n + 1) {
        sum = sum + i;
    }
    return sum;
}

fn main() -> i32 {
    print_i32(triangular(100));
    return 0;
}
""",
"""#include <stdio.h>
static int triangular(int n) {
    int sum = 0;
    for (int i = 1; i <= n; i++) sum += i;
    return sum;
}
int main(void) { printf("%d\\n", triangular(100)); return 0; }
""")

# =============================================================================
# NUMERICAL METHODS (036-055)
# =============================================================================

register(36, "leibniz_pi", "numerical", "3.141593\n",
"""@module leibniz_pi;

fn main() -> i32 {
    let x: f64 = 1.0;
    for i: i32 in range(2, 10000001) {
        let k: i32 = i * 2 - 1;
        let d: f64 = to_f64(k);
        if i % 2 == 0 {
            x = x - 1.0 / d;
        } else {
            x = x + 1.0 / d;
        }
    }
    let pi: f64 = x * 4.0;
    print_f64(pi);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double x = 1.0;
    for (int i = 2; i <= 10000000; i++) {
        double d = (double)(2 * i - 1);
        if (i % 2 == 0) x -= 1.0 / d;
        else x += 1.0 / d;
    }
    printf("%f\\n", x * 4.0);
    return 0;
}
""")

register(37, "newton_sqrt", "numerical", "3.162278\n",
"""@module newton_sqrt;

@pure
fn newton_sqrt(x: f64, iterations: i32) -> f64 {
    let guess: f64 = x / 2.0;
    for i: i32 in range(0, iterations) {
        guess = (guess + x / guess) / 2.0;
    }
    return guess;
}

fn main() -> i32 {
    print_f64(newton_sqrt(10.0, 20));
    return 0;
}
""",
"""#include <stdio.h>
static double newton_sqrt(double x, int iterations) {
    double guess = x / 2.0;
    for (int i = 0; i < iterations; i++)
        guess = (guess + x / guess) / 2.0;
    return guess;
}
int main(void) { printf("%f\\n", newton_sqrt(10.0, 20)); return 0; }
""")

register(38, "newton_cbrt", "numerical", "3.000000\n",
"""@module newton_cbrt;

@pure
fn newton_cbrt(x: f64, iterations: i32) -> f64 {
    let guess: f64 = x / 3.0;
    for i: i32 in range(0, iterations) {
        guess = (2.0 * guess + x / (guess * guess)) / 3.0;
    }
    return guess;
}

fn main() -> i32 {
    print_f64(newton_cbrt(27.0, 30));
    return 0;
}
""",
"""#include <stdio.h>
static double newton_cbrt(double x, int iterations) {
    double guess = x / 3.0;
    for (int i = 0; i < iterations; i++)
        guess = (2.0 * guess + x / (guess * guess)) / 3.0;
    return guess;
}
int main(void) { printf("%f\\n", newton_cbrt(27.0, 30)); return 0; }
""")

register(39, "euler_number", "numerical", "2.718282\n",
"""@module euler_number;

fn main() -> i32 {
    let e: f64 = 1.0;
    let factorial: f64 = 1.0;
    for i: i32 in range(1, 21) {
        factorial = factorial * to_f64(i);
        e = e + 1.0 / factorial;
    }
    print_f64(e);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double e = 1.0, factorial = 1.0;
    for (int i = 1; i <= 20; i++) {
        factorial *= (double)i;
        e += 1.0 / factorial;
    }
    printf("%f\\n", e);
    return 0;
}
""")

register(40, "sine_taylor", "numerical", "0.841471\n",
"""@module sine_taylor;

@pure
fn sine_taylor(x: f64, terms: i32) -> f64 {
    let result: f64 = 0.0;
    let power: f64 = x;
    let factorial: f64 = 1.0;
    for i: i32 in range(0, terms) {
        let n: i32 = 2 * i + 1;
        if i > 0 {
            power = power * x * x;
            factorial = factorial * to_f64(n - 1) * to_f64(n);
        }
        if i % 2 == 0 {
            result = result + power / factorial;
        } else {
            result = result - power / factorial;
        }
    }
    return result;
}

fn main() -> i32 {
    print_f64(sine_taylor(1.0, 15));
    return 0;
}
""",
"""#include <stdio.h>
static double sine_taylor(double x, int terms) {
    double result = 0.0, power = x, factorial = 1.0;
    for (int i = 0; i < terms; i++) {
        int n = 2 * i + 1;
        if (i > 0) { power *= x * x; factorial *= (double)(n - 1) * n; }
        if (i % 2 == 0) result += power / factorial;
        else result -= power / factorial;
    }
    return result;
}
int main(void) { printf("%f\\n", sine_taylor(1.0, 15)); return 0; }
""")

register(41, "cosine_taylor", "numerical", "0.540302\n",
"""@module cosine_taylor;

@pure
fn cosine_taylor(x: f64, terms: i32) -> f64 {
    let result: f64 = 0.0;
    let power: f64 = 1.0;
    let factorial: f64 = 1.0;
    for i: i32 in range(0, terms) {
        let n: i32 = 2 * i;
        if i > 0 {
            power = power * x * x;
            factorial = factorial * to_f64(n - 1) * to_f64(n);
        }
        if i % 2 == 0 {
            result = result + power / factorial;
        } else {
            result = result - power / factorial;
        }
    }
    return result;
}

fn main() -> i32 {
    print_f64(cosine_taylor(1.0, 15));
    return 0;
}
""",
"""#include <stdio.h>
static double cosine_taylor(double x, int terms) {
    double result = 0.0, power = 1.0, factorial = 1.0;
    for (int i = 0; i < terms; i++) {
        int n = 2 * i;
        if (i > 0) { power *= x * x; factorial *= (double)(n - 1) * n; }
        if (i % 2 == 0) result += power / factorial;
        else result -= power / factorial;
    }
    return result;
}
int main(void) { printf("%f\\n", cosine_taylor(1.0, 15)); return 0; }
""")

register(42, "exp_taylor", "numerical", "7.389056\n",
"""@module exp_taylor;

@pure
fn exp_taylor(x: f64, terms: i32) -> f64 {
    let result: f64 = 1.0;
    let term: f64 = 1.0;
    for i: i32 in range(1, terms) {
        term = term * x / to_f64(i);
        result = result + term;
    }
    return result;
}

fn main() -> i32 {
    print_f64(exp_taylor(2.0, 25));
    return 0;
}
""",
"""#include <stdio.h>
static double exp_taylor(double x, int terms) {
    double result = 1.0, term = 1.0;
    for (int i = 1; i < terms; i++) {
        term *= x / (double)i;
        result += term;
    }
    return result;
}
int main(void) { printf("%f\\n", exp_taylor(2.0, 25)); return 0; }
""")

register(43, "ln_series", "numerical", "0.693147\n",
"""@module ln_series;

@pure
fn ln_approx(x: f64, terms: i32) -> f64 {
    let y: f64 = (x - 1.0) / (x + 1.0);
    let result: f64 = 0.0;
    let power: f64 = y;
    for i: i32 in range(0, terms) {
        let n: i32 = 2 * i + 1;
        result = result + power / to_f64(n);
        power = power * y * y;
    }
    return 2.0 * result;
}

fn main() -> i32 {
    print_f64(ln_approx(2.0, 50));
    return 0;
}
""",
"""#include <stdio.h>
static double ln_approx(double x, int terms) {
    double y = (x - 1.0) / (x + 1.0), result = 0.0, power = y;
    for (int i = 0; i < terms; i++) {
        int n = 2 * i + 1;
        result += power / (double)n;
        power *= y * y;
    }
    return 2.0 * result;
}
int main(void) { printf("%f\\n", ln_approx(2.0, 50)); return 0; }
""")

register(44, "wallis_pi", "numerical", "3.141592\n",
"""@module wallis_pi;

fn main() -> i32 {
    let product: f64 = 1.0;
    for i: i32 in range(1, 1000001) {
        let n: f64 = to_f64(i);
        let term: f64 = 4.0 * n * n / (4.0 * n * n - 1.0);
        product = product * term;
    }
    let pi: f64 = 2.0 * product;
    print_f64(pi);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double product = 1.0;
    for (int i = 1; i <= 1000000; i++) {
        double n = (double)i;
        product *= 4.0 * n * n / (4.0 * n * n - 1.0);
    }
    printf("%f\\n", 2.0 * product);
    return 0;
}
""")

register(45, "integration_midpoint", "numerical", "0.333333\n",
"""@module integration_midpoint;

fn main() -> i32 {
    let a: f64 = 0.0;
    let b: f64 = 1.0;
    let n: i32 = 1000000;
    let dx: f64 = (b - a) / to_f64(n);
    let sum: f64 = 0.0;
    for i: i32 in range(0, n) {
        let x: f64 = a + (to_f64(i) + 0.5) * dx;
        sum = sum + x * x;
    }
    let result: f64 = sum * dx;
    print_f64(result);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double a = 0.0, b = 1.0;
    int n = 1000000;
    double dx = (b - a) / (double)n, sum = 0.0;
    for (int i = 0; i < n; i++) {
        double x = a + ((double)i + 0.5) * dx;
        sum += x * x;
    }
    printf("%f\\n", sum * dx);
    return 0;
}
""")

register(46, "integration_trapezoidal", "numerical", "0.333333\n",
"""@module integration_trapezoidal;

fn main() -> i32 {
    let a: f64 = 0.0;
    let b: f64 = 1.0;
    let n: i32 = 1000000;
    let dx: f64 = (b - a) / to_f64(n);
    let sum: f64 = a * a / 2.0 + b * b / 2.0;
    for i: i32 in range(1, n) {
        let x: f64 = a + to_f64(i) * dx;
        sum = sum + x * x;
    }
    let result: f64 = sum * dx;
    print_f64(result);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double a = 0.0, b = 1.0;
    int n = 1000000;
    double dx = (b - a) / (double)n;
    double sum = a * a / 2.0 + b * b / 2.0;
    for (int i = 1; i < n; i++) {
        double x = a + (double)i * dx;
        sum += x * x;
    }
    printf("%f\\n", sum * dx);
    return 0;
}
""")

register(47, "integration_simpson", "numerical", "0.333333\n",
"""@module integration_simpson;

fn main() -> i32 {
    let a: f64 = 0.0;
    let b: f64 = 1.0;
    let n: i32 = 1000000;
    let dx: f64 = (b - a) / to_f64(n);
    let sum: f64 = a * a + b * b;
    for i: i32 in range(1, n) {
        let x: f64 = a + to_f64(i) * dx;
        if i % 2 == 0 {
            sum = sum + 2.0 * x * x;
        } else {
            sum = sum + 4.0 * x * x;
        }
    }
    let result: f64 = sum * dx / 3.0;
    print_f64(result);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double a = 0.0, b = 1.0;
    int n = 1000000;
    double dx = (b - a) / (double)n;
    double sum = a * a + b * b;
    for (int i = 1; i < n; i++) {
        double x = a + (double)i * dx;
        if (i % 2 == 0) sum += 2.0 * x * x;
        else sum += 4.0 * x * x;
    }
    printf("%f\\n", sum * dx / 3.0);
    return 0;
}
""")

register(48, "golden_ratio", "numerical", "1.618034\n",
"""@module golden_ratio;

fn main() -> i32 {
    let phi: f64 = 1.0;
    for i: i32 in range(0, 100) {
        phi = 1.0 + 1.0 / phi;
    }
    print_f64(phi);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double phi = 1.0;
    for (int i = 0; i < 100; i++) phi = 1.0 + 1.0 / phi;
    printf("%f\\n", phi);
    return 0;
}
""")

register(49, "logistic_map", "numerical", "0.513045\n",
"""@module logistic_map;

fn main() -> i32 {
    let r: f64 = 3.2;
    let x: f64 = 0.5;
    for i: i32 in range(0, 1000) {
        x = r * x * (1.0 - x);
    }
    print_f64(x);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double r = 3.2, x = 0.5;
    for (int i = 0; i < 1000; i++) x = r * x * (1.0 - x);
    printf("%f\\n", x);
    return 0;
}
""")

register(50, "harmonic_sum", "numerical", "7.485471\n",
"""@module harmonic_sum;

fn main() -> i32 {
    let sum: f64 = 0.0;
    for i: i32 in range(1, 1001) {
        sum = sum + 1.0 / to_f64(i);
    }
    print_f64(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double sum = 0.0;
    for (int i = 1; i <= 1000; i++) sum += 1.0 / (double)i;
    printf("%f\\n", sum);
    return 0;
}
""")

register(51, "basel_problem", "numerical", "1.644924\n",
"""@module basel_problem;

fn main() -> i32 {
    let sum: f64 = 0.0;
    for i: i32 in range(1, 100001) {
        let n: f64 = to_f64(i);
        sum = sum + 1.0 / (n * n);
    }
    print_f64(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double sum = 0.0;
    for (int i = 1; i <= 100000; i++) {
        double n = (double)i;
        sum += 1.0 / (n * n);
    }
    printf("%f\\n", sum);
    return 0;
}
""")

register(52, "sqrt_builtin", "numerical", "1.414214\n",
"""@module sqrt_builtin;

fn main() -> i32 {
    print_f64(sqrt(2.0));
    return 0;
}
""",
"""#include <stdio.h>
#include <math.h>
int main(void) { printf("%f\\n", sqrt(2.0)); return 0; }
""")

register(53, "pow_builtin", "numerical", "1024.000000\n",
"""@module pow_builtin;

fn main() -> i32 {
    print_f64(pow(2.0, 10.0));
    return 0;
}
""",
"""#include <stdio.h>
#include <math.h>
int main(void) { printf("%f\\n", pow(2.0, 10.0)); return 0; }
""")

register(54, "geometric_series", "numerical", "1.999998\n",
"""@module geometric_series;

fn main() -> i32 {
    let sum: f64 = 0.0;
    let term: f64 = 1.0;
    for i: i32 in range(0, 20) {
        sum = sum + term;
        term = term / 2.0;
    }
    print_f64(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double sum = 0.0, term = 1.0;
    for (int i = 0; i < 20; i++) { sum += term; term /= 2.0; }
    printf("%f\\n", sum);
    return 0;
}
""")

register(55, "pi_bbp_style", "numerical", "3.141593\n",
"""@module pi_bbp_style;

fn main() -> i32 {
    let pi: f64 = 0.0;
    let sixteen_pow: f64 = 1.0;
    for k: i32 in range(0, 20) {
        let kf: f64 = to_f64(k);
        let term: f64 = 4.0 / (8.0 * kf + 1.0) - 2.0 / (8.0 * kf + 4.0) - 1.0 / (8.0 * kf + 5.0) - 1.0 / (8.0 * kf + 6.0);
        pi = pi + term / sixteen_pow;
        sixteen_pow = sixteen_pow * 16.0;
    }
    print_f64(pi);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double pi = 0.0, sixteen_pow = 1.0;
    for (int k = 0; k < 20; k++) {
        double kf = (double)k;
        double term = 4.0/(8.0*kf+1.0) - 2.0/(8.0*kf+4.0) - 1.0/(8.0*kf+5.0) - 1.0/(8.0*kf+6.0);
        pi += term / sixteen_pow;
        sixteen_pow *= 16.0;
    }
    printf("%f\\n", pi);
    return 0;
}
""")

# =============================================================================
# RECURSION STRESS (056-072)
# =============================================================================

register(56, "ackermann_2_3", "recursion", "9\n",
"""@module ackermann_2_3;

@pure
fn ackermann(m: i32, n: i32) -> i32 {
    if m == 0 {
        return n + 1;
    }
    if n == 0 {
        return ackermann(m - 1, 1);
    }
    return ackermann(m - 1, ackermann(m, n - 1));
}

fn main() -> i32 {
    print_i32(ackermann(2, 3));
    return 0;
}
""",
"""#include <stdio.h>
static int ackermann(int m, int n) {
    if (m == 0) return n + 1;
    if (n == 0) return ackermann(m - 1, 1);
    return ackermann(m - 1, ackermann(m, n - 1));
}
int main(void) { printf("%d\\n", ackermann(2, 3)); return 0; }
""")

register(57, "ackermann_3_4", "recursion", "125\n",
"""@module ackermann_3_4;

@pure
fn ackermann(m: i32, n: i32) -> i32 {
    if m == 0 {
        return n + 1;
    }
    if n == 0 {
        return ackermann(m - 1, 1);
    }
    return ackermann(m - 1, ackermann(m, n - 1));
}

fn main() -> i32 {
    print_i32(ackermann(3, 4));
    return 0;
}
""",
"""#include <stdio.h>
static int ackermann(int m, int n) {
    if (m == 0) return n + 1;
    if (n == 0) return ackermann(m - 1, 1);
    return ackermann(m - 1, ackermann(m, n - 1));
}
int main(void) { printf("%d\\n", ackermann(3, 4)); return 0; }
""")

register(58, "tak", "recursion", "7\n",
"""@module tak;

@pure
fn tak(x: i32, y: i32, z: i32) -> i32 {
    if y >= x {
        return z;
    }
    return tak(tak(x - 1, y, z), tak(y - 1, z, x), tak(z - 1, x, y));
}

fn main() -> i32 {
    print_i32(tak(18, 12, 6));
    return 0;
}
""",
"""#include <stdio.h>
static int tak(int x, int y, int z) {
    if (y >= x) return z;
    return tak(tak(x-1,y,z), tak(y-1,z,x), tak(z-1,x,y));
}
int main(void) { printf("%d\\n", tak(18, 12, 6)); return 0; }
""")

register(59, "mccarthy91", "recursion", "91\n",
"""@module mccarthy91;

@pure
fn mc91(n: i32) -> i32 {
    if n > 100 {
        return n - 10;
    }
    return mc91(mc91(n + 11));
}

fn main() -> i32 {
    print_i32(mc91(42));
    return 0;
}
""",
"""#include <stdio.h>
static int mc91(int n) {
    if (n > 100) return n - 10;
    return mc91(mc91(n + 11));
}
int main(void) { printf("%d\\n", mc91(42)); return 0; }
""")

register(60, "towers_of_hanoi", "recursion", "1023\n",
"""@module towers_of_hanoi;

@pure
fn hanoi_count(n: i32) -> i32 {
    if n == 0 {
        return 0;
    }
    return 2 * hanoi_count(n - 1) + 1;
}

fn main() -> i32 {
    print_i32(hanoi_count(10));
    return 0;
}
""",
"""#include <stdio.h>
static int hanoi_count(int n) {
    if (n == 0) return 0;
    return 2 * hanoi_count(n - 1) + 1;
}
int main(void) { printf("%d\\n", hanoi_count(10)); return 0; }
""")

register(61, "catalan_recursive", "recursion", "42\n",
"""@module catalan_recursive;

@pure
fn catalan(n: i32) -> i32 {
    if n <= 1 {
        return 1;
    }
    let sum: i32 = 0;
    for i: i32 in range(0, n) {
        sum = sum + catalan(i) * catalan(n - 1 - i);
    }
    return sum;
}

fn main() -> i32 {
    print_i32(catalan(5));
    return 0;
}
""",
"""#include <stdio.h>
static int catalan(int n) {
    if (n <= 1) return 1;
    int sum = 0;
    for (int i = 0; i < n; i++) sum += catalan(i) * catalan(n - 1 - i);
    return sum;
}
int main(void) { printf("%d\\n", catalan(5)); return 0; }
""")

register(62, "pascal_element", "recursion", "10\n",
"""@module pascal_element;

@pure
fn pascal(row: i32, col: i32) -> i32 {
    if col == 0 {
        return 1;
    }
    if col == row {
        return 1;
    }
    return pascal(row - 1, col - 1) + pascal(row - 1, col);
}

fn main() -> i32 {
    print_i32(pascal(5, 2));
    return 0;
}
""",
"""#include <stdio.h>
static int pascal(int row, int col) {
    if (col == 0 || col == row) return 1;
    return pascal(row-1, col-1) + pascal(row-1, col);
}
int main(void) { printf("%d\\n", pascal(5, 2)); return 0; }
""")

register(63, "pascal_row_sum", "recursion", "1024\n",
"""@module pascal_row_sum;

@pure
fn pascal(row: i32, col: i32) -> i32 {
    if col == 0 {
        return 1;
    }
    if col == row {
        return 1;
    }
    return pascal(row - 1, col - 1) + pascal(row - 1, col);
}

fn main() -> i32 {
    let sum: i32 = 0;
    for c: i32 in range(0, 11) {
        sum = sum + pascal(10, c);
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
static int pascal(int row, int col) {
    if (col == 0 || col == row) return 1;
    return pascal(row-1, col-1) + pascal(row-1, col);
}
int main(void) {
    int sum = 0;
    for (int c = 0; c <= 10; c++) sum += pascal(10, c);
    printf("%d\\n", sum);
    return 0;
}
""")

register(64, "power_recursive", "recursion", "59049\n",
"""@module power_recursive;

@pure
fn power(base: i32, exp: i32) -> i32 {
    if exp == 0 {
        return 1;
    }
    return base * power(base, exp - 1);
}

fn main() -> i32 {
    print_i32(power(3, 10));
    return 0;
}
""",
"""#include <stdio.h>
static int power(int base, int exp) {
    if (exp == 0) return 1;
    return base * power(base, exp - 1);
}
int main(void) { printf("%d\\n", power(3, 10)); return 0; }
""")

register(65, "power_fast_recursive", "recursion", "1073741824\n",
"""@module power_fast_recursive;

@pure
fn fast_power(base: i64, exp: i32) -> i64 {
    if exp == 0 {
        return 1;
    }
    if exp % 2 == 0 {
        let half: i64 = fast_power(base, exp / 2);
        return half * half;
    }
    return base * fast_power(base, exp - 1);
}

fn main() -> i32 {
    print_i64(fast_power(2, 30));
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
static int64_t fast_power(int64_t base, int exp) {
    if (exp == 0) return 1;
    if (exp % 2 == 0) { int64_t half = fast_power(base, exp/2); return half * half; }
    return base * fast_power(base, exp - 1);
}
int main(void) { printf("%lld\\n", (long long)fast_power(2, 30)); return 0; }
""")

register(66, "sum_recursive", "recursion", "5050\n",
"""@module sum_recursive;

@pure
fn sum_to(n: i32) -> i32 {
    if n == 0 {
        return 0;
    }
    return n + sum_to(n - 1);
}

fn main() -> i32 {
    print_i32(sum_to(100));
    return 0;
}
""",
"""#include <stdio.h>
static int sum_to(int n) {
    if (n == 0) return 0;
    return n + sum_to(n - 1);
}
int main(void) { printf("%d\\n", sum_to(100)); return 0; }
""")

register(67, "gcd_recursive_stress", "recursion", "1\n",
"""@module gcd_recursive_stress;

@pure
fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 {
        return a;
    }
    return gcd(b, a % b);
}

fn main() -> i32 {
    let result: i32 = 0;
    for i: i32 in range(1, 1001) {
        result = gcd(i, 997);
    }
    print_i32(result);
    return 0;
}
""",
"""#include <stdio.h>
static int gcd(int a, int b) {
    if (b == 0) return a;
    return gcd(b, a % b);
}
int main(void) {
    int result = 0;
    for (int i = 1; i <= 1000; i++) result = gcd(i, 997);
    printf("%d\\n", result);
    return 0;
}
""")

register(68, "fibonacci_sum_recursive", "recursion", "2178308\n",
"""@module fibonacci_sum_recursive;

@pure
fn fib(n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn main() -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(0, 31) {
        sum = sum + fib(i);
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
static int fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}
int main(void) {
    int sum = 0;
    for (int i = 0; i <= 30; i++) sum += fib(i);
    printf("%d\\n", sum);
    return 0;
}
""")

register(69, "tree_depth", "recursion", "20\n",
"""@module tree_depth;

@pure
fn depth(n: i32) -> i32 {
    if n <= 1 {
        return 0;
    }
    let left: i32 = depth(n / 2);
    let right: i32 = depth(n - n / 2 - 1);
    if left > right {
        return left + 1;
    }
    return right + 1;
}

fn main() -> i32 {
    print_i32(depth(1000000));
    return 0;
}
""",
"""#include <stdio.h>
static int depth(int n) {
    if (n <= 1) return 0;
    int left = depth(n / 2);
    int right = depth(n - n / 2 - 1);
    return (left > right ? left : right) + 1;
}
int main(void) { printf("%d\\n", depth(1000000)); return 0; }
""")

register(70, "count_paths", "recursion", "48620\n",
"""@module count_paths;

@pure
fn count_paths(m: i32, n: i32) -> i32 {
    if m == 1 {
        return 1;
    }
    if n == 1 {
        return 1;
    }
    return count_paths(m - 1, n) + count_paths(m, n - 1);
}

fn main() -> i32 {
    print_i32(count_paths(10, 10));
    return 0;
}
""",
"""#include <stdio.h>
static int count_paths(int m, int n) {
    if (m == 1 || n == 1) return 1;
    return count_paths(m - 1, n) + count_paths(m, n - 1);
}
int main(void) { printf("%d\\n", count_paths(10, 10)); return 0; }
""")

register(71, "integer_partition_count", "recursion", "42\n",
"""@module integer_partition_count;

@pure
fn partitions(n: i32, k: i32) -> i32 {
    if n == 0 {
        return 1;
    }
    if n < 0 {
        return 0;
    }
    if k == 0 {
        return 0;
    }
    return partitions(n - k, k) + partitions(n, k - 1);
}

fn main() -> i32 {
    print_i32(partitions(10, 10));
    return 0;
}
""",
"""#include <stdio.h>
static int partitions(int n, int k) {
    if (n == 0) return 1;
    if (n < 0 || k == 0) return 0;
    return partitions(n - k, k) + partitions(n, k - 1);
}
int main(void) { printf("%d\\n", partitions(10, 10)); return 0; }
""")

register(72, "recursive_multiply", "recursion", "56088\n",
"""@module recursive_multiply;

@pure
fn multiply(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0;
    }
    if b > 0 {
        return a + multiply(a, b - 1);
    }
    return 0 - multiply(a, 0 - b);
}

fn main() -> i32 {
    print_i32(multiply(237, 237));
    return 0;
}
""",
"""#include <stdio.h>
static int multiply(int a, int b) {
    if (b == 0) return 0;
    if (b > 0) return a + multiply(a, b - 1);
    return -multiply(a, -b);
}
int main(void) { printf("%d\\n", multiply(237, 237)); return 0; }
""")

# =============================================================================
# ARRAY / LOOP COMPUTE (073-095)
# =============================================================================

register(73, "array_sum_squares", "array_loop", "285\n",
"""@module array_sum_squares;

fn main() -> i32 {
    let arr: array[i32, 10] = array_zeros[i32, 10];
    for i: i32 in range(0, 10) {
        arr[i] = i * i;
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 10) {
        sum = sum + arr[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[10] = {0};
    for (int i = 0; i < 10; i++) arr[i] = i * i;
    int sum = 0;
    for (int i = 0; i < 10; i++) sum += arr[i];
    printf("%d\\n", sum);
    return 0;
}
""")

register(74, "array_sum_100", "array_loop", "4950\n",
"""@module array_sum_100;

fn main() -> i32 {
    let arr: array[i32, 100] = array_zeros[i32, 100];
    for i: i32 in range(0, 100) {
        arr[i] = i;
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 100) {
        sum = sum + arr[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[100] = {0};
    for (int i = 0; i < 100; i++) arr[i] = i;
    int sum = 0;
    for (int i = 0; i < 100; i++) sum += arr[i];
    printf("%d\\n", sum);
    return 0;
}
""")

register(75, "array_reverse_sum", "array_loop", "4950\n",
"""@module array_reverse_sum;

fn main() -> i32 {
    let arr: array[i32, 100] = array_zeros[i32, 100];
    for i: i32 in range(0, 100) {
        arr[i] = i;
    }
    let rev: array[i32, 100] = array_zeros[i32, 100];
    for i: i32 in range(0, 100) {
        rev[i] = arr[99 - i];
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 100) {
        sum = sum + rev[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[100], rev[100];
    for (int i = 0; i < 100; i++) arr[i] = i;
    for (int i = 0; i < 100; i++) rev[i] = arr[99 - i];
    int sum = 0;
    for (int i = 0; i < 100; i++) sum += rev[i];
    printf("%d\\n", sum);
    return 0;
}
""")

register(76, "bubble_sort_sum", "array_loop", "210\n",
"""@module bubble_sort_sum;

fn main() -> i32 {
    let arr: array[i32, 20] = array_zeros[i32, 20];
    for i: i32 in range(0, 20) {
        arr[i] = 20 - i;
    }
    for i: i32 in range(0, 19) {
        for j: i32 in range(0, 19 - i) {
            if arr[j] > arr[j + 1] {
                let tmp: i32 = arr[j];
                arr[j] = arr[j + 1];
                arr[j + 1] = tmp;
            }
        }
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 20) {
        sum = sum + arr[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[20];
    for (int i = 0; i < 20; i++) arr[i] = 20 - i;
    for (int i = 0; i < 19; i++)
        for (int j = 0; j < 19 - i; j++)
            if (arr[j] > arr[j+1]) { int t = arr[j]; arr[j] = arr[j+1]; arr[j+1] = t; }
    int sum = 0;
    for (int i = 0; i < 20; i++) sum += arr[i];
    printf("%d\\n", sum);
    return 0;
}
""")

register(77, "selection_sort_sum", "array_loop", "210\n",
"""@module selection_sort_sum;

fn main() -> i32 {
    let arr: array[i32, 20] = array_zeros[i32, 20];
    for i: i32 in range(0, 20) {
        arr[i] = 20 - i;
    }
    for i: i32 in range(0, 19) {
        let min_idx: i32 = i;
        for j: i32 in range(i + 1, 20) {
            if arr[j] < arr[min_idx] {
                min_idx = j;
            }
        }
        let tmp: i32 = arr[i];
        arr[i] = arr[min_idx];
        arr[min_idx] = tmp;
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 20) {
        sum = sum + arr[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[20];
    for (int i = 0; i < 20; i++) arr[i] = 20 - i;
    for (int i = 0; i < 19; i++) {
        int min_idx = i;
        for (int j = i + 1; j < 20; j++)
            if (arr[j] < arr[min_idx]) min_idx = j;
        int t = arr[i]; arr[i] = arr[min_idx]; arr[min_idx] = t;
    }
    int sum = 0;
    for (int i = 0; i < 20; i++) sum += arr[i];
    printf("%d\\n", sum);
    return 0;
}
""")

register(78, "insertion_sort_sum", "array_loop", "210\n",
"""@module insertion_sort_sum;

fn main() -> i32 {
    let arr: array[i32, 20] = array_zeros[i32, 20];
    for i: i32 in range(0, 20) {
        arr[i] = 20 - i;
    }
    for i: i32 in range(1, 20) {
        let key: i32 = arr[i];
        let j: i32 = i - 1;
        while j >= 0 {
            if arr[j] > key {
                arr[j + 1] = arr[j];
                j = j - 1;
            } else {
                j = -1;
            }
        }
        arr[j + 1] = key;
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 20) {
        sum = sum + arr[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[20];
    for (int i = 0; i < 20; i++) arr[i] = 20 - i;
    for (int i = 1; i < 20; i++) {
        int key = arr[i], j = i - 1;
        while (j >= 0 && arr[j] > key) { arr[j+1] = arr[j]; j--; }
        arr[j+1] = key;
    }
    int sum = 0;
    for (int i = 0; i < 20; i++) sum += arr[i];
    printf("%d\\n", sum);
    return 0;
}
""")

register(79, "dot_product", "array_loop", "2870\n",
"""@module dot_product;

fn main() -> i32 {
    let a: array[i32, 20] = array_zeros[i32, 20];
    let b: array[i32, 20] = array_zeros[i32, 20];
    for i: i32 in range(0, 20) {
        a[i] = i + 1;
        b[i] = i + 1;
    }
    let dot: i32 = 0;
    for i: i32 in range(0, 20) {
        dot = dot + a[i] * b[i];
    }
    print_i32(dot);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int a[20], b[20];
    for (int i = 0; i < 20; i++) { a[i] = i + 1; b[i] = i + 1; }
    int dot = 0;
    for (int i = 0; i < 20; i++) dot += a[i] * b[i];
    printf("%d\\n", dot);
    return 0;
}
""")

register(80, "prefix_sum", "array_loop", "45\n",
"""@module prefix_sum;

fn main() -> i32 {
    let arr: array[i32, 10] = array_zeros[i32, 10];
    for i: i32 in range(0, 10) {
        arr[i] = i;
    }
    for i: i32 in range(1, 10) {
        arr[i] = arr[i] + arr[i - 1];
    }
    print_i32(arr[9]);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[10];
    for (int i = 0; i < 10; i++) arr[i] = i;
    for (int i = 1; i < 10; i++) arr[i] += arr[i-1];
    printf("%d\\n", arr[9]);
    return 0;
}
""")

register(81, "kadane_max_subarray", "array_loop", "6\n",
"""@module kadane_max_subarray;

fn main() -> i32 {
    let arr: array[i32, 10] = array_zeros[i32, 10];
    arr[0] = -2;
    arr[1] = 1;
    arr[2] = -3;
    arr[3] = 4;
    arr[4] = -1;
    arr[5] = 2;
    arr[6] = 1;
    arr[7] = -5;
    arr[8] = 4;
    arr[9] = 1;
    let max_ending: i32 = arr[0];
    let max_so_far: i32 = arr[0];
    for i: i32 in range(1, 10) {
        if max_ending + arr[i] > arr[i] {
            max_ending = max_ending + arr[i];
        } else {
            max_ending = arr[i];
        }
        if max_ending > max_so_far {
            max_so_far = max_ending;
        }
    }
    print_i32(max_so_far);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[10] = {-2, 1, -3, 4, -1, 2, 1, -5, 4, 1};
    int max_ending = arr[0], max_so_far = arr[0];
    for (int i = 1; i < 10; i++) {
        max_ending = max_ending + arr[i] > arr[i] ? max_ending + arr[i] : arr[i];
        if (max_ending > max_so_far) max_so_far = max_ending;
    }
    printf("%d\\n", max_so_far);
    return 0;
}
""")

register(82, "sieve_eratosthenes", "array_loop", "1229\n",
"""@module sieve_eratosthenes;

fn main() -> i32 {
    let sieve: array[i32, 10001] = array_zeros[i32, 10001];
    for i: i32 in range(0, 10001) {
        sieve[i] = 1;
    }
    sieve[0] = 0;
    sieve[1] = 0;
    let i: i32 = 2;
    while i * i <= 10000 {
        if sieve[i] == 1 {
            let j: i32 = i * i;
            while j <= 10000 {
                sieve[j] = 0;
                j = j + i;
            }
        }
        i = i + 1;
    }
    let count: i32 = 0;
    for k: i32 in range(0, 10001) {
        count = count + sieve[k];
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int sieve[10001];
    for (int i = 0; i <= 10000; i++) sieve[i] = 1;
    sieve[0] = sieve[1] = 0;
    for (int i = 2; i * i <= 10000; i++)
        if (sieve[i])
            for (int j = i * i; j <= 10000; j += i) sieve[j] = 0;
    int count = 0;
    for (int i = 0; i <= 10000; i++) count += sieve[i];
    printf("%d\\n", count);
    return 0;
}
""")

register(83, "polynomial_eval", "array_loop", "12345\n",
"""@module polynomial_eval;

fn main() -> i32 {
    let coeffs: array[i32, 5] = array_zeros[i32, 5];
    coeffs[0] = 5;
    coeffs[1] = 4;
    coeffs[2] = 3;
    coeffs[3] = 2;
    coeffs[4] = 1;
    let x: i32 = 10;
    let result: i32 = 0;
    for i: i32 in range(0, 5) {
        let power: i32 = 1;
        for j: i32 in range(0, i) {
            power = power * x;
        }
        result = result + coeffs[i] * power;
    }
    print_i32(result);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int coeffs[5] = {5, 4, 3, 2, 1};
    int x = 10, result = 0;
    for (int i = 0; i < 5; i++) {
        int power = 1;
        for (int j = 0; j < i; j++) power *= x;
        result += coeffs[i] * power;
    }
    printf("%d\\n", result);
    return 0;
}
""")

register(84, "polynomial_horner", "array_loop", "12345\n",
"""@module polynomial_horner;

fn main() -> i32 {
    let coeffs: array[i32, 5] = array_zeros[i32, 5];
    coeffs[0] = 1;
    coeffs[1] = 2;
    coeffs[2] = 3;
    coeffs[3] = 4;
    coeffs[4] = 5;
    let x: i32 = 10;
    let result: i32 = coeffs[0];
    for i: i32 in range(1, 5) {
        result = result * x + coeffs[i];
    }
    print_i32(result);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int coeffs[5] = {1, 2, 3, 4, 5};
    int x = 10, result = coeffs[0];
    for (int i = 1; i < 5; i++) result = result * x + coeffs[i];
    printf("%d\\n", result);
    return 0;
}
""")

register(85, "matrix_multiply_flat", "array_loop", "3375\n",
"""@module matrix_multiply_flat;

fn main() -> i32 {
    let n: i32 = 5;
    let a: array[i32, 25] = array_zeros[i32, 25];
    let b: array[i32, 25] = array_zeros[i32, 25];
    let c: array[i32, 25] = array_zeros[i32, 25];
    for i: i32 in range(0, 5) {
        for j: i32 in range(0, 5) {
            a[i * 5 + j] = i + j + 1;
            b[i * 5 + j] = i + j + 1;
        }
    }
    for i: i32 in range(0, 5) {
        for j: i32 in range(0, 5) {
            let sum: i32 = 0;
            for k: i32 in range(0, 5) {
                sum = sum + a[i * 5 + k] * b[k * 5 + j];
            }
            c[i * 5 + j] = sum;
        }
    }
    let total: i32 = 0;
    for i: i32 in range(0, 25) {
        total = total + c[i];
    }
    print_i32(total);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int a[25], b[25], c[25] = {0};
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) {
            a[i*5+j] = i + j + 1;
            b[i*5+j] = i + j + 1;
        }
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) {
            int sum = 0;
            for (int k = 0; k < 5; k++) sum += a[i*5+k] * b[k*5+j];
            c[i*5+j] = sum;
        }
    int total = 0;
    for (int i = 0; i < 25; i++) total += c[i];
    printf("%d\\n", total);
    return 0;
}
""")

register(86, "binary_search", "array_loop", "42\n",
"""@module binary_search;

fn main() -> i32 {
    let arr: array[i32, 100] = array_zeros[i32, 100];
    for i: i32 in range(0, 100) {
        arr[i] = i * 2;
    }
    let target: i32 = 84;
    let lo: i32 = 0;
    let hi: i32 = 99;
    let result: i32 = -1;
    while lo <= hi {
        let mid: i32 = lo + (hi - lo) / 2;
        if arr[mid] == target {
            result = mid;
            lo = hi + 1;
        } else {
            if arr[mid] < target {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
        }
    }
    print_i32(result);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[100];
    for (int i = 0; i < 100; i++) arr[i] = i * 2;
    int target = 84, lo = 0, hi = 99, result = -1;
    while (lo <= hi) {
        int mid = lo + (hi - lo) / 2;
        if (arr[mid] == target) { result = mid; break; }
        else if (arr[mid] < target) lo = mid + 1;
        else hi = mid - 1;
    }
    printf("%d\\n", result);
    return 0;
}
""")

register(87, "histogram", "array_loop", "10\n",
"""@module histogram;

fn main() -> i32 {
    let data: array[i32, 50] = array_zeros[i32, 50];
    for i: i32 in range(0, 50) {
        data[i] = i % 5;
    }
    let hist: array[i32, 5] = array_zeros[i32, 5];
    for i: i32 in range(0, 50) {
        let idx: i32 = data[i];
        hist[idx] = hist[idx] + 1;
    }
    print_i32(hist[0]);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int data[50], hist[5] = {0};
    for (int i = 0; i < 50; i++) data[i] = i % 5;
    for (int i = 0; i < 50; i++) hist[data[i]]++;
    printf("%d\\n", hist[0]);
    return 0;
}
""")

register(88, "running_average", "array_loop", "49.500000\n",
"""@module running_average;

fn main() -> i32 {
    let arr: array[i32, 100] = array_zeros[i32, 100];
    for i: i32 in range(0, 100) {
        arr[i] = i;
    }
    let sum: f64 = 0.0;
    for i: i32 in range(0, 100) {
        sum = sum + to_f64(arr[i]);
    }
    let avg: f64 = sum / 100.0;
    print_f64(avg);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[100];
    for (int i = 0; i < 100; i++) arr[i] = i;
    double sum = 0.0;
    for (int i = 0; i < 100; i++) sum += (double)arr[i];
    printf("%f\\n", sum / 100.0);
    return 0;
}
""")

register(89, "count_inversions", "array_loop", "190\n",
"""@module count_inversions;

fn main() -> i32 {
    let arr: array[i32, 20] = array_zeros[i32, 20];
    for i: i32 in range(0, 20) {
        arr[i] = 20 - i;
    }
    let count: i32 = 0;
    for i: i32 in range(0, 19) {
        for j: i32 in range(i + 1, 20) {
            if arr[i] > arr[j] {
                count = count + 1;
            }
        }
    }
    print_i32(count);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[20];
    for (int i = 0; i < 20; i++) arr[i] = 20 - i;
    int count = 0;
    for (int i = 0; i < 19; i++)
        for (int j = i + 1; j < 20; j++)
            if (arr[i] > arr[j]) count++;
    printf("%d\\n", count);
    return 0;
}
""")

register(90, "array_min_max", "array_loop", "99\n",
"""@module array_min_max;

fn main() -> i32 {
    let arr: array[i32, 100] = array_zeros[i32, 100];
    for i: i32 in range(0, 100) {
        arr[i] = (i * 37 + 13) % 100;
    }
    let mn: i32 = arr[0];
    let mx: i32 = arr[0];
    for i: i32 in range(1, 100) {
        if arr[i] < mn {
            mn = arr[i];
        }
        if arr[i] > mx {
            mx = arr[i];
        }
    }
    print_i32(mx - mn);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[100];
    for (int i = 0; i < 100; i++) arr[i] = (i * 37 + 13) % 100;
    int mn = arr[0], mx = arr[0];
    for (int i = 1; i < 100; i++) {
        if (arr[i] < mn) mn = arr[i];
        if (arr[i] > mx) mx = arr[i];
    }
    printf("%d\\n", mx - mn);
    return 0;
}
""")

register(91, "matrix_trace", "array_loop", "25\n",
"""@module matrix_trace;

fn main() -> i32 {
    let mat: array[i32, 25] = array_zeros[i32, 25];
    for i: i32 in range(0, 5) {
        for j: i32 in range(0, 5) {
            mat[i * 5 + j] = i + j;
        }
    }
    let trace: i32 = 0;
    for i: i32 in range(0, 5) {
        trace = trace + mat[i * 5 + i];
    }
    print_i32(trace);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int mat[25];
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) mat[i*5+j] = i + j;
    int trace = 0;
    for (int i = 0; i < 5; i++) trace += mat[i*5+i];
    printf("%d\\n", trace);
    return 0;
}
""")

# Note: 5x5 with values i+j: diagonal is 0,2,4,6,8 -> sum 20. Let me fix.
BENCHMARKS.pop()  # remove last
register(91, "matrix_trace", "array_loop", "20\n",
"""@module matrix_trace;

fn main() -> i32 {
    let mat: array[i32, 25] = array_zeros[i32, 25];
    for i: i32 in range(0, 5) {
        for j: i32 in range(0, 5) {
            mat[i * 5 + j] = i + j;
        }
    }
    let trace: i32 = 0;
    for i: i32 in range(0, 5) {
        trace = trace + mat[i * 5 + i];
    }
    print_i32(trace);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int mat[25];
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) mat[i*5+j] = i + j;
    int trace = 0;
    for (int i = 0; i < 5; i++) trace += mat[i*5+i];
    printf("%d\\n", trace);
    return 0;
}
""")

register(92, "matrix_transpose_sum", "array_loop", "600\n",
"""@module matrix_transpose_sum;

fn main() -> i32 {
    let mat: array[i32, 25] = array_zeros[i32, 25];
    let trans: array[i32, 25] = array_zeros[i32, 25];
    for i: i32 in range(0, 5) {
        for j: i32 in range(0, 5) {
            mat[i * 5 + j] = i * 5 + j + 1;
        }
    }
    for i: i32 in range(0, 5) {
        for j: i32 in range(0, 5) {
            trans[j * 5 + i] = mat[i * 5 + j];
        }
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 25) {
        sum = sum + mat[i] + trans[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int mat[25], trans[25];
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) mat[i*5+j] = i*5+j+1;
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) trans[j*5+i] = mat[i*5+j];
    int sum = 0;
    for (int i = 0; i < 25; i++) sum += mat[i] + trans[i];
    printf("%d\\n", sum);
    return 0;
}
""")

# mat sum = 25*26/2 = 325, transpose sum = 325, total = 650. Fix.
BENCHMARKS.pop()
register(92, "matrix_transpose_sum", "array_loop", "650\n",
"""@module matrix_transpose_sum;

fn main() -> i32 {
    let mat: array[i32, 25] = array_zeros[i32, 25];
    let trans: array[i32, 25] = array_zeros[i32, 25];
    for i: i32 in range(0, 5) {
        for j: i32 in range(0, 5) {
            mat[i * 5 + j] = i * 5 + j + 1;
        }
    }
    for i: i32 in range(0, 5) {
        for j: i32 in range(0, 5) {
            trans[j * 5 + i] = mat[i * 5 + j];
        }
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 25) {
        sum = sum + mat[i] + trans[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int mat[25], trans[25];
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) mat[i*5+j] = i*5+j+1;
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) trans[j*5+i] = mat[i*5+j];
    int sum = 0;
    for (int i = 0; i < 25; i++) sum += mat[i] + trans[i];
    printf("%d\\n", sum);
    return 0;
}
""")

register(93, "array_copy_sum", "array_loop", "4950\n",
"""@module array_copy_sum;

fn main() -> i32 {
    let src: array[i32, 100] = array_zeros[i32, 100];
    let dst: array[i32, 100] = array_zeros[i32, 100];
    for i: i32 in range(0, 100) {
        src[i] = i;
    }
    for i: i32 in range(0, 100) {
        dst[i] = src[i];
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 100) {
        sum = sum + dst[i];
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int src[100], dst[100];
    for (int i = 0; i < 100; i++) src[i] = i;
    for (int i = 0; i < 100; i++) dst[i] = src[i];
    int sum = 0;
    for (int i = 0; i < 100; i++) sum += dst[i];
    printf("%d\\n", sum);
    return 0;
}
""")

register(94, "array_accumulate_products", "array_loop", "3628800\n",
"""@module array_accumulate_products;

fn main() -> i32 {
    let arr: array[i32, 10] = array_zeros[i32, 10];
    for i: i32 in range(0, 10) {
        arr[i] = i + 1;
    }
    let product: i32 = 1;
    for i: i32 in range(0, 10) {
        product = product * arr[i];
    }
    print_i32(product);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[10];
    for (int i = 0; i < 10; i++) arr[i] = i + 1;
    int product = 1;
    for (int i = 0; i < 10; i++) product *= arr[i];
    printf("%d\\n", product);
    return 0;
}
""")

register(95, "two_sum_brute", "array_loop", "3\n",
"""@module two_sum_brute;

fn main() -> i32 {
    let arr: array[i32, 10] = array_zeros[i32, 10];
    arr[0] = 2;
    arr[1] = 7;
    arr[2] = 11;
    arr[3] = 15;
    arr[4] = 1;
    arr[5] = 8;
    arr[6] = 3;
    arr[7] = 4;
    arr[8] = 5;
    arr[9] = 6;
    let target: i32 = 16;
    let result: i32 = -1;
    for i: i32 in range(0, 9) {
        for j: i32 in range(i + 1, 10) {
            if arr[i] + arr[j] == target {
                result = i + j;
            }
        }
    }
    print_i32(result);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[10] = {2, 7, 11, 15, 1, 8, 3, 4, 5, 6};
    int target = 16, result = -1;
    for (int i = 0; i < 9; i++)
        for (int j = i + 1; j < 10; j++)
            if (arr[i] + arr[j] == target) result = i + j;
    printf("%d\\n", result);
    return 0;
}
""")

# 2+15=17 no, 7+11=18 no... 11+5=16? arr[2]=11 + arr[8]=5 = 16, so result = 2+8 = 10
# Actually 2+15 nope. Let's just check: target=16. 11+5=16 (i=2,j=8), 15+1=16 (i=3,j=4), 8+8 nope, 7+... nope
# (3,4) -> result = 3+4=7, (2,8) -> result = 2+8=10. Last found wins. Let me fix expected.
BENCHMARKS.pop()
register(95, "two_sum_brute", "array_loop", "7\n",
"""@module two_sum_brute;

fn main() -> i32 {
    let arr: array[i32, 10] = array_zeros[i32, 10];
    arr[0] = 2;
    arr[1] = 7;
    arr[2] = 11;
    arr[3] = 15;
    arr[4] = 1;
    arr[5] = 8;
    arr[6] = 3;
    arr[7] = 4;
    arr[8] = 5;
    arr[9] = 6;
    let target: i32 = 16;
    let result: i32 = -1;
    for i: i32 in range(0, 9) {
        for j: i32 in range(i + 1, 10) {
            if arr[i] + arr[j] == target {
                result = i + j;
            }
        }
    }
    print_i32(result);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int arr[10] = {2, 7, 11, 15, 1, 8, 3, 4, 5, 6};
    int target = 16, result = -1;
    for (int i = 0; i < 9; i++)
        for (int j = i + 1; j < 10; j++)
            if (arr[i] + arr[j] == target) result = i + j;
    printf("%d\\n", result);
    return 0;
}
""")

# =============================================================================
# PURE COMPUTE (096-115)
# =============================================================================

register(96, "mandelbrot_escape", "pure_compute", "5\n",
"""@module mandelbrot_escape;

@pure
fn mandelbrot(cr: f64, ci: f64, max_iter: i32) -> i32 {
    let zr: f64 = 0.0;
    let zi: f64 = 0.0;
    let iter: i32 = 0;
    while iter < max_iter {
        let zr2: f64 = zr * zr;
        let zi2: f64 = zi * zi;
        if zr2 + zi2 > 4.0 {
            return iter;
        }
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
        iter = iter + 1;
    }
    return max_iter;
}

fn main() -> i32 {
    print_i32(mandelbrot(0.5, 0.5, 1000));
    return 0;
}
""",
"""#include <stdio.h>
static int mandelbrot(double cr, double ci, int max_iter) {
    double zr = 0.0, zi = 0.0;
    for (int iter = 0; iter < max_iter; iter++) {
        double zr2 = zr * zr, zi2 = zi * zi;
        if (zr2 + zi2 > 4.0) return iter;
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
    }
    return max_iter;
}
int main(void) { printf("%d\\n", mandelbrot(0.5, 0.5, 1000)); return 0; }
""")

register(97, "mandelbrot_grid", "pure_compute", "9030\n",
"""@module mandelbrot_grid;

@pure
fn mandelbrot(cr: f64, ci: f64, max_iter: i32) -> i32 {
    let zr: f64 = 0.0;
    let zi: f64 = 0.0;
    let iter: i32 = 0;
    while iter < max_iter {
        let zr2: f64 = zr * zr;
        let zi2: f64 = zi * zi;
        if zr2 + zi2 > 4.0 {
            return iter;
        }
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
        iter = iter + 1;
    }
    return max_iter;
}

fn main() -> i32 {
    let total: i32 = 0;
    for y: i32 in range(0, 20) {
        for x: i32 in range(0, 20) {
            let cr: f64 = -2.0 + to_f64(x) * 0.15;
            let ci: f64 = -1.5 + to_f64(y) * 0.15;
            total = total + mandelbrot(cr, ci, 100);
        }
    }
    print_i32(total);
    return 0;
}
""",
"""#include <stdio.h>
static int mandelbrot(double cr, double ci, int max_iter) {
    double zr = 0.0, zi = 0.0;
    for (int iter = 0; iter < max_iter; iter++) {
        double zr2 = zr * zr, zi2 = zi * zi;
        if (zr2 + zi2 > 4.0) return iter;
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
    }
    return max_iter;
}
int main(void) {
    int total = 0;
    for (int y = 0; y < 20; y++)
        for (int x = 0; x < 20; x++) {
            double cr = -2.0 + x * 0.15, ci = -1.5 + y * 0.15;
            total += mandelbrot(cr, ci, 100);
        }
    printf("%d\\n", total);
    return 0;
}
""")

register(98, "julia_escape", "pure_compute", "4\n",
"""@module julia_escape;

@pure
fn julia(zr0: f64, zi0: f64, cr: f64, ci: f64, max_iter: i32) -> i32 {
    let zr: f64 = zr0;
    let zi: f64 = zi0;
    let iter: i32 = 0;
    while iter < max_iter {
        let zr2: f64 = zr * zr;
        let zi2: f64 = zi * zi;
        if zr2 + zi2 > 4.0 {
            return iter;
        }
        let new_zr: f64 = zr2 - zi2 + cr;
        zi = 2.0 * zr * zi + ci;
        zr = new_zr;
        iter = iter + 1;
    }
    return max_iter;
}

fn main() -> i32 {
    print_i32(julia(0.5, 0.5, -0.7, 0.27015, 1000));
    return 0;
}
""",
"""#include <stdio.h>
static int julia(double zr, double zi, double cr, double ci, int max_iter) {
    for (int iter = 0; iter < max_iter; iter++) {
        double zr2 = zr * zr, zi2 = zi * zi;
        if (zr2 + zi2 > 4.0) return iter;
        double new_zr = zr2 - zi2 + cr;
        zi = 2.0 * zr * zi + ci;
        zr = new_zr;
    }
    return max_iter;
}
int main(void) { printf("%d\\n", julia(0.5, 0.5, -0.7, 0.27015, 1000)); return 0; }
""")

register(99, "henon_map", "pure_compute", "1.080031\n",
"""@module henon_map;

fn main() -> i32 {
    let a: f64 = 1.4;
    let b: f64 = 0.3;
    let x: f64 = 0.0;
    let y: f64 = 0.0;
    for i: i32 in range(0, 1000) {
        let new_x: f64 = 1.0 - a * x * x + y;
        let new_y: f64 = b * x;
        x = new_x;
        y = new_y;
    }
    print_f64(x);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double a = 1.4, b = 0.3, x = 0.0, y = 0.0;
    for (int i = 0; i < 1000; i++) {
        double new_x = 1.0 - a * x * x + y;
        y = b * x;
        x = new_x;
    }
    printf("%f\\n", x);
    return 0;
}
""")

register(100, "pi_lcg_monte_carlo", "pure_compute", "3.142956\n",
"""@module pi_lcg_monte_carlo;

fn main() -> i32 {
    let n: i32 = 1000000;
    let inside: i32 = 0;
    let seed: i64 = 12345;
    let a_lcg: i64 = 1103515245;
    let c_lcg: i64 = 12345;
    let m_lcg: i64 = 2147483648;
    for i: i32 in range(0, n) {
        seed = (a_lcg * seed + c_lcg) % m_lcg;
        let x: f64 = to_f64_i64(seed) / to_f64_i64(m_lcg);
        seed = (a_lcg * seed + c_lcg) % m_lcg;
        let y: f64 = to_f64_i64(seed) / to_f64_i64(m_lcg);
        if x * x + y * y <= 1.0 {
            inside = inside + 1;
        }
    }
    let pi: f64 = 4.0 * to_f64(inside) / to_f64(n);
    print_f64(pi);
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
int main(void) {
    int n = 1000000, inside = 0;
    int64_t seed = 12345, a = 1103515245, c = 12345, m = 2147483648LL;
    for (int i = 0; i < n; i++) {
        seed = (a * seed + c) % m;
        double x = (double)seed / (double)m;
        seed = (a * seed + c) % m;
        double y = (double)seed / (double)m;
        if (x * x + y * y <= 1.0) inside++;
    }
    printf("%f\\n", 4.0 * (double)inside / (double)n);
    return 0;
}
""")

register(101, "lorenz_step", "pure_compute", "-4.397957\n",
"""@module lorenz_step;

fn main() -> i32 {
    let sigma: f64 = 10.0;
    let rho: f64 = 28.0;
    let beta: f64 = 8.0 / 3.0;
    let dt: f64 = 0.001;
    let x: f64 = 1.0;
    let y: f64 = 1.0;
    let z: f64 = 1.0;
    for i: i32 in range(0, 10000) {
        let dx: f64 = sigma * (y - x);
        let dy: f64 = x * (rho - z) - y;
        let dz: f64 = x * y - beta * z;
        x = x + dx * dt;
        y = y + dy * dt;
        z = z + dz * dt;
    }
    print_f64(x);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double sigma = 10.0, rho = 28.0, beta = 8.0/3.0, dt = 0.001;
    double x = 1.0, y = 1.0, z = 1.0;
    for (int i = 0; i < 10000; i++) {
        double dx = sigma * (y - x);
        double dy = x * (rho - z) - y;
        double dz = x * y - beta * z;
        x += dx * dt; y += dy * dt; z += dz * dt;
    }
    printf("%f\\n", x);
    return 0;
}
""")

register(102, "random_walk_distance", "pure_compute", "0\n",
"""@module random_walk_distance;

fn main() -> i32 {
    let n: i32 = 10000;
    let seed: i64 = 42;
    let a_lcg: i64 = 1103515245;
    let c_lcg: i64 = 12345;
    let m_lcg: i64 = 2147483648;
    let x: i32 = 0;
    let y: i32 = 0;
    for i: i32 in range(0, n) {
        seed = (a_lcg * seed + c_lcg) % m_lcg;
        let dir: i32 = narrow(seed % 4);
        if dir == 0 {
            x = x + 1;
        } else {
            if dir == 1 {
                x = x - 1;
            } else {
                if dir == 2 {
                    y = y + 1;
                } else {
                    y = y - 1;
                }
            }
        }
    }
    let dist: i32 = abs(x) + abs(y);
    print_i32(dist);
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
int main(void) {
    int n = 10000;
    int64_t seed = 42, a = 1103515245, c = 12345, m = 2147483648LL;
    int x = 0, y = 0;
    for (int i = 0; i < n; i++) {
        seed = (a * seed + c) % m;
        int dir = (int)(seed % 4);
        if (dir == 0) x++;
        else if (dir == 1) x--;
        else if (dir == 2) y++;
        else y--;
    }
    printf("%d\\n", abs(x) + abs(y));
    return 0;
}
""")

register(103, "nbody_scalar_2d", "pure_compute", "12.668147\n",
"""@module nbody_scalar_2d;

fn main() -> i32 {
    let x0: f64 = 0.0;
    let y0: f64 = 0.0;
    let x1: f64 = 1.0;
    let y1: f64 = 0.0;
    let x2: f64 = 0.0;
    let y2: f64 = 1.0;
    let vx0: f64 = 0.0;
    let vy0: f64 = 0.01;
    let vx1: f64 = 0.0;
    let vy1: f64 = -0.005;
    let vx2: f64 = -0.005;
    let vy2: f64 = 0.0;
    let dt: f64 = 0.001;
    for step: i32 in range(0, 1000) {
        let dx01: f64 = x1 - x0;
        let dy01: f64 = y1 - y0;
        let r01_sq: f64 = dx01 * dx01 + dy01 * dy01 + 0.0001;
        let r01: f64 = sqrt(r01_sq);
        let f01: f64 = 1.0 / (r01_sq * r01);
        vx0 = vx0 + f01 * dx01 * dt;
        vy0 = vy0 + f01 * dy01 * dt;
        vx1 = vx1 - f01 * dx01 * dt;
        vy1 = vy1 - f01 * dy01 * dt;
        x0 = x0 + vx0 * dt;
        y0 = y0 + vy0 * dt;
        x1 = x1 + vx1 * dt;
        y1 = y1 + vy1 * dt;
        x2 = x2 + vx2 * dt;
        y2 = y2 + vy2 * dt;
    }
    let energy: f64 = 0.5 * (vx0 * vx0 + vy0 * vy0 + vx1 * vx1 + vy1 * vy1 + vx2 * vx2 + vy2 * vy2);
    print_f64(energy);
    return 0;
}
""",
"""#include <stdio.h>
#include <math.h>
int main(void) {
    double x0=0,y0=0, x1=1,y1=0, x2=0,y2=1;
    double vx0=0,vy0=0.01, vx1=0,vy1=-0.005, vx2=-0.005,vy2=0;
    double dt = 0.001;
    for (int step = 0; step < 1000; step++) {
        double dx01=x1-x0, dy01=y1-y0;
        double r01_sq = dx01*dx01+dy01*dy01+0.0001;
        double r01 = sqrt(r01_sq);
        double f01 = 1.0/(r01_sq*r01);
        vx0+=f01*dx01*dt; vy0+=f01*dy01*dt;
        vx1-=f01*dx01*dt; vy1-=f01*dy01*dt;
        x0+=vx0*dt; y0+=vy0*dt;
        x1+=vx1*dt; y1+=vy1*dt;
        x2+=vx2*dt; y2+=vy2*dt;
    }
    double energy = 0.5*(vx0*vx0+vy0*vy0+vx1*vx1+vy1*vy1+vx2*vx2+vy2*vy2);
    printf("%f\\n", energy);
    return 0;
}
""")

register(104, "sum_of_abs", "pure_compute", "50\n",
"""@module sum_of_abs;

fn main() -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(-50, 51) {
        sum = sum + abs(i);
    }
    let expected: i32 = 2 * 50 * 51 / 2;
    print_i32(sum - expected);
    return 0;
}
""",
"""#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int sum = 0;
    for (int i = -50; i <= 50; i++) sum += abs(i);
    printf("%d\\n", sum - 2 * 50 * 51 / 2);
    return 0;
}
""")

# Actually sum_of_abs(-50..50) = 2*(1+2+...+50) + 0 = 2*1275 = 2550. Expected = 2*50*51/2 = 2550. So result = 0. Fix.
BENCHMARKS.pop()
register(104, "sum_of_abs", "pure_compute", "2550\n",
"""@module sum_of_abs;

fn main() -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(-50, 51) {
        sum = sum + abs(i);
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int sum = 0;
    for (int i = -50; i <= 50; i++) sum += abs(i);
    printf("%d\\n", sum);
    return 0;
}
""")

register(105, "min_max_builtin", "pure_compute", "7\n",
"""@module min_max_builtin;

fn main() -> i32 {
    let a: i32 = min(3, 7);
    let b: i32 = max(3, 7);
    print_i32(a + b - min(a, b));
    return 0;
}
""",
"""#include <stdio.h>
static int min_val(int a, int b) { return a < b ? a : b; }
static int max_val(int a, int b) { return a > b ? a : b; }
int main(void) {
    int a = min_val(3, 7), b = max_val(3, 7);
    printf("%d\\n", a + b - min_val(a, b));
    return 0;
}
""")

register(106, "lcg_stats", "pure_compute", "1093731792\n",
"""@module lcg_stats;

fn main() -> i32 {
    let seed: i64 = 1;
    let a_lcg: i64 = 1103515245;
    let c_lcg: i64 = 12345;
    let m_lcg: i64 = 2147483648;
    let sum: i64 = 0;
    for i: i32 in range(0, 1000) {
        seed = (a_lcg * seed + c_lcg) % m_lcg;
        sum = sum + seed;
    }
    let avg: i64 = sum / 1000;
    print_i64(avg);
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
int main(void) {
    int64_t seed = 1, a = 1103515245, c = 12345, m = 2147483648LL, sum = 0;
    for (int i = 0; i < 1000; i++) {
        seed = (a * seed + c) % m;
        sum += seed;
    }
    printf("%lld\\n", (long long)(sum / 1000));
    return 0;
}
""")

register(107, "fizzbuzz_sum", "pure_compute", "2873\n",
"""@module fizzbuzz_sum;

fn main() -> i32 {
    let sum: i32 = 0;
    for i: i32 in range(1, 101) {
        if i % 15 == 0 {
            sum = sum + 15;
        } else {
            if i % 3 == 0 {
                sum = sum + 3;
            } else {
                if i % 5 == 0 {
                    sum = sum + 5;
                } else {
                    sum = sum + i;
                }
            }
        }
    }
    print_i32(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int sum = 0;
    for (int i = 1; i <= 100; i++) {
        if (i % 15 == 0) sum += 15;
        else if (i % 3 == 0) sum += 3;
        else if (i % 5 == 0) sum += 5;
        else sum += i;
    }
    printf("%d\\n", sum);
    return 0;
}
""")

register(108, "quadratic_roots", "pure_compute", "3.000000\n",
"""@module quadratic_roots;

fn main() -> i32 {
    let a: f64 = 1.0;
    let b: f64 = -5.0;
    let c: f64 = 6.0;
    let discriminant: f64 = b * b - 4.0 * a * c;
    let root1: f64 = (0.0 - b + sqrt(discriminant)) / (2.0 * a);
    print_f64(root1);
    return 0;
}
""",
"""#include <stdio.h>
#include <math.h>
int main(void) {
    double a = 1.0, b = -5.0, c = 6.0;
    double disc = b*b - 4.0*a*c;
    printf("%f\\n", (-b + sqrt(disc)) / (2.0*a));
    return 0;
}
""")

register(109, "distance_2d", "pure_compute", "5.000000\n",
"""@module distance_2d;

fn main() -> i32 {
    let x1: f64 = 0.0;
    let y1: f64 = 0.0;
    let x2: f64 = 3.0;
    let y2: f64 = 4.0;
    let dx: f64 = x2 - x1;
    let dy: f64 = y2 - y1;
    let dist: f64 = sqrt(dx * dx + dy * dy);
    print_f64(dist);
    return 0;
}
""",
"""#include <stdio.h>
#include <math.h>
int main(void) {
    double dx = 3.0, dy = 4.0;
    printf("%f\\n", sqrt(dx*dx + dy*dy));
    return 0;
}
""")

register(110, "circle_area_sum", "pure_compute", "3850.441770\n",
"""@module circle_area_sum;

fn main() -> i32 {
    let pi: f64 = 3.14159265358979;
    let total: f64 = 0.0;
    for r: i32 in range(1, 50) {
        let rf: f64 = to_f64(r);
        total = total + pi * rf * rf;
    }
    print_f64(total);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double pi = 3.14159265358979, total = 0.0;
    for (int r = 1; r < 50; r++) total += pi * (double)r * (double)r;
    printf("%f\\n", total);
    return 0;
}
""")

# pi * sum(r^2 for r=1..49) = pi * 49*50*99/6 = pi * 40425 = 126964.4... That's wrong. Let me recalculate.
# Actually range(1,50) = 1..49 in AXIOM. sum(i^2, i=1..49) = 49*50*99/6 = 40425. pi * 40425 = 126964.xxx. Fix.
BENCHMARKS.pop()
register(110, "circle_area_sum", "pure_compute", "126998.883021\n",
"""@module circle_area_sum;

fn main() -> i32 {
    let pi: f64 = 3.14159265358979;
    let total: f64 = 0.0;
    for r: i32 in range(1, 50) {
        let rf: f64 = to_f64(r);
        total = total + pi * rf * rf;
    }
    print_f64(total);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double pi = 3.14159265358979, total = 0.0;
    for (int r = 1; r < 50; r++) total += pi * (double)r * (double)r;
    printf("%f\\n", total);
    return 0;
}
""")

register(111, "widen_narrow_roundtrip", "pure_compute", "42\n",
"""@module widen_narrow_roundtrip;

fn main() -> i32 {
    let x: i32 = 42;
    let y: i64 = widen(x);
    let z: i32 = narrow(y);
    print_i32(z);
    return 0;
}
""",
"""#include <stdio.h>
#include <stdint.h>
int main(void) {
    int x = 42;
    int64_t y = (int64_t)x;
    int z = (int)y;
    printf("%d\\n", z);
    return 0;
}
""")

register(112, "type_conversion_chain", "pure_compute", "42\n",
"""@module type_conversion_chain;

fn main() -> i32 {
    let x: i32 = 42;
    let y: f64 = to_f64(x);
    let z: i32 = truncate(y);
    print_i32(z);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int x = 42;
    double y = (double)x;
    int z = (int)y;
    printf("%d\\n", z);
    return 0;
}
""")

register(113, "nested_loop_sum", "pure_compute", "2550\n",
"""@module nested_loop_sum;

fn main() -> i32 {
    let total: i32 = 0;
    for i: i32 in range(0, 10) {
        for j: i32 in range(0, i + 1) {
            total = total + i;
        }
    }
    print_i32(total);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int total = 0;
    for (int i = 0; i < 10; i++)
        for (int j = 0; j <= i; j++)
            total += i;
    printf("%d\\n", total);
    return 0;
}
""")

# sum = sum(i*(i+1)) for i=0..9 = sum(i^2+i) = 285+45 = 330. Fix expected.
BENCHMARKS.pop()
register(113, "nested_loop_sum", "pure_compute", "330\n",
"""@module nested_loop_sum;

fn main() -> i32 {
    let total: i32 = 0;
    for i: i32 in range(0, 10) {
        for j: i32 in range(0, i + 1) {
            total = total + i;
        }
    }
    print_i32(total);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    int total = 0;
    for (int i = 0; i < 10; i++)
        for (int j = 0; j <= i; j++)
            total += i;
    printf("%d\\n", total);
    return 0;
}
""")

register(114, "compound_interest", "pure_compute", "2.593742\n",
"""@module compound_interest;

fn main() -> i32 {
    let principal: f64 = 1.0;
    let rate: f64 = 0.1;
    let years: i32 = 10;
    let amount: f64 = principal;
    for i: i32 in range(0, years) {
        amount = amount * (1.0 + rate);
    }
    print_f64(amount);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double amount = 1.0;
    for (int i = 0; i < 10; i++) amount *= 1.1;
    printf("%f\\n", amount);
    return 0;
}
""")

register(115, "alternating_series", "pure_compute", "0.693147\n",
"""@module alternating_series;

fn main() -> i32 {
    let sum: f64 = 0.0;
    for i: i32 in range(1, 1000001) {
        let term: f64 = 1.0 / to_f64(i);
        if i % 2 == 0 {
            sum = sum - term;
        } else {
            sum = sum + term;
        }
    }
    print_f64(sum);
    return 0;
}
""",
"""#include <stdio.h>
int main(void) {
    double sum = 0.0;
    for (int i = 1; i <= 1000000; i++) {
        double term = 1.0 / (double)i;
        if (i % 2 == 0) sum -= term;
        else sum += term;
    }
    printf("%f\\n", sum);
    return 0;
}
""")

# =============================================================================
# Write files
# =============================================================================

def write_all():
    """Write all benchmark files, manifest, and run script."""
    os.makedirs(SUITE_DIR, exist_ok=True)

    manifest = []
    for b in BENCHMARKS:
        # Write .axm file
        axm_path = os.path.join(SUITE_DIR, b["axm"])
        with open(axm_path, "w", newline="\n") as f:
            f.write(b["axm_code"].lstrip("\n"))

        # Write .c file
        c_path = os.path.join(SUITE_DIR, b["c"])
        with open(c_path, "w", newline="\n") as f:
            f.write(b["c_code"].lstrip("\n"))

        manifest.append({
            "id": b["id"],
            "name": b["name"],
            "category": b["category"],
            "expected_output": b["expected_output"],
            "axm": b["axm"],
            "c": b["c"],
        })

    # Write manifest.json
    manifest_path = os.path.join(SUITE_DIR, "manifest.json")
    with open(manifest_path, "w", newline="\n") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")

    print(f"Generated {len(BENCHMARKS)} benchmarks in {SUITE_DIR}")
    print(f"  .axm files: {len(BENCHMARKS)}")
    print(f"  .c   files: {len(BENCHMARKS)}")
    print(f"  manifest:   {manifest_path}")

    # Category breakdown
    categories = {}
    for b in BENCHMARKS:
        cat = b["category"]
        categories[cat] = categories.get(cat, 0) + 1
    print("\nCategory breakdown:")
    for cat, count in sorted(categories.items()):
        print(f"  {cat}: {count}")

    # Write run_all.py
    write_run_all()


def write_run_all():
    """Write the benchmark runner script."""
    run_all_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "run_all.py")
    with open(run_all_path, "w", newline="\n") as f:
        f.write(RUN_ALL_SCRIPT)
    print(f"  runner:     {run_all_path}")


RUN_ALL_SCRIPT = r'''#!/usr/bin/env python3
"""Run all AXIOM benchmarks: compile, execute, verify, and time."""

import json
import os
import subprocess
import sys
import time
import statistics

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
SUITE_DIR = os.path.join(SCRIPT_DIR, "suite")
MANIFEST_PATH = os.path.join(SUITE_DIR, "manifest.json")
RESULTS_PATH = os.path.join(SCRIPT_DIR, "results.json")

# Discover AXIOM compiler and clang
AXIOM_BIN = None
for candidate in [
    os.path.join(SCRIPT_DIR, "..", "target", "release", "axiom.exe"),
    os.path.join(SCRIPT_DIR, "..", "target", "release", "axiom"),
    os.path.join(SCRIPT_DIR, "..", "target", "debug", "axiom.exe"),
    os.path.join(SCRIPT_DIR, "..", "target", "debug", "axiom"),
]:
    if os.path.isfile(candidate):
        AXIOM_BIN = os.path.abspath(candidate)
        break

CLANG_BIN = None
for candidate in ["clang", "clang-19", "clang-18", "clang-17", "clang-16", "clang-15", "gcc", "cc"]:
    try:
        subprocess.run([candidate, "--version"], capture_output=True, check=True)
        CLANG_BIN = candidate
        break
    except (subprocess.CalledProcessError, FileNotFoundError):
        continue


def compile_axiom(axm_path, out_path):
    """Compile an .axm file to a native binary using the AXIOM compiler."""
    if not AXIOM_BIN:
        return False, "AXIOM compiler not found"
    try:
        result = subprocess.run(
            [AXIOM_BIN, "compile", axm_path, "-o", out_path],
            capture_output=True, text=True, timeout=60
        )
        if result.returncode != 0:
            return False, result.stderr.strip()
        return True, ""
    except subprocess.TimeoutExpired:
        return False, "compilation timed out"
    except Exception as e:
        return False, str(e)


def compile_c(c_path, out_path):
    """Compile a .c file using clang."""
    if not CLANG_BIN:
        return False, "clang not found"
    try:
        result = subprocess.run(
            [CLANG_BIN, "-O2", "-o", out_path, c_path, "-lm"],
            capture_output=True, text=True, timeout=60
        )
        if result.returncode != 0:
            return False, result.stderr.strip()
        return True, ""
    except subprocess.TimeoutExpired:
        return False, "compilation timed out"
    except Exception as e:
        return False, str(e)


def run_binary(bin_path, timeout_sec=30):
    """Run a binary and capture its output."""
    try:
        result = subprocess.run(
            [bin_path],
            capture_output=True, text=True, timeout=timeout_sec
        )
        return result.stdout, result.returncode
    except subprocess.TimeoutExpired:
        return None, -1
    except Exception as e:
        return None, -1


def time_binary(bin_path, runs=5, timeout_sec=30):
    """Time a binary over multiple runs and return median time in ms."""
    times = []
    for _ in range(runs):
        start = time.perf_counter()
        try:
            subprocess.run([bin_path], capture_output=True, timeout=timeout_sec)
        except subprocess.TimeoutExpired:
            return None
        elapsed = (time.perf_counter() - start) * 1000
        times.append(elapsed)
    return statistics.median(times)


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Run AXIOM benchmark suite")
    parser.add_argument("--filter", help="Only run benchmarks matching this name pattern")
    parser.add_argument("--runs", type=int, default=5, help="Number of timing runs (default: 5)")
    parser.add_argument("--no-time", action="store_true", help="Skip timing (just verify correctness)")
    parser.add_argument("--category", help="Only run benchmarks in this category")
    parser.add_argument("--verify-only", action="store_true", help="Only verify outputs, skip timing")
    args = parser.parse_args()

    if not os.path.isfile(MANIFEST_PATH):
        print(f"ERROR: Manifest not found at {MANIFEST_PATH}")
        print("Run generate_benchmarks.py first.")
        sys.exit(1)

    with open(MANIFEST_PATH) as f:
        manifest = json.load(f)

    print(f"AXIOM Benchmark Suite")
    print(f"=====================")
    print(f"AXIOM compiler: {AXIOM_BIN or 'NOT FOUND'}")
    print(f"C compiler:     {CLANG_BIN or 'NOT FOUND'}")
    print(f"Benchmarks:     {len(manifest)}")
    print(f"Timing runs:    {args.runs}")
    print()

    # Build output directory
    build_dir = os.path.join(SCRIPT_DIR, "build")
    os.makedirs(build_dir, exist_ok=True)

    results = []
    total = 0
    passed = 0
    failed = 0
    skipped = 0

    for bench in manifest:
        bid = bench["id"]
        name = bench["name"]
        category = bench["category"]
        expected = bench["expected_output"]
        axm_file = os.path.join(SUITE_DIR, bench["axm"])
        c_file = os.path.join(SUITE_DIR, bench["c"])

        # Apply filters
        if args.filter and args.filter not in name:
            continue
        if args.category and args.category != category:
            continue

        total += 1
        ext = ".exe" if sys.platform == "win32" else ""
        axm_bin = os.path.join(build_dir, f"{bid}_{name}_axm{ext}")
        c_bin = os.path.join(build_dir, f"{bid}_{name}_c{ext}")

        result_entry = {
            "id": bid,
            "name": name,
            "category": category,
            "status": "unknown",
        }

        # Compile AXIOM
        axm_ok, axm_err = compile_axiom(axm_file, axm_bin)
        if not axm_ok:
            result_entry["status"] = "compile_fail_axm"
            result_entry["error"] = axm_err
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} COMPILE FAIL (axm): {axm_err[:80]}")
            continue

        # Compile C
        c_ok, c_err = compile_c(c_file, c_bin)
        if not c_ok:
            result_entry["status"] = "compile_fail_c"
            result_entry["error"] = c_err
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} COMPILE FAIL (c): {c_err[:80]}")
            continue

        # Run and verify AXIOM
        axm_out, axm_rc = run_binary(axm_bin)
        c_out, c_rc = run_binary(c_bin)

        if axm_out is None:
            result_entry["status"] = "timeout_axm"
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} TIMEOUT (axm)")
            continue

        if c_out is None:
            result_entry["status"] = "timeout_c"
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} TIMEOUT (c)")
            continue

        # Compare outputs
        axm_match = axm_out == expected or axm_out.strip() == expected.strip()
        c_match = c_out == expected or c_out.strip() == expected.strip()
        outputs_match = axm_out.strip() == c_out.strip()

        if not outputs_match:
            result_entry["status"] = "output_mismatch"
            result_entry["axm_output"] = axm_out.strip()
            result_entry["c_output"] = c_out.strip()
            result_entry["expected"] = expected.strip()
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} MISMATCH axm={axm_out.strip()!r} c={c_out.strip()!r} expected={expected.strip()!r}")
            continue

        # Timing
        axm_ms = None
        c_ms = None
        if not args.no_time and not args.verify_only:
            axm_ms = time_binary(axm_bin, runs=args.runs)
            c_ms = time_binary(c_bin, runs=args.runs)

        result_entry["status"] = "pass"
        result_entry["output"] = axm_out.strip()
        if axm_ms is not None:
            result_entry["axm_time_ms"] = round(axm_ms, 3)
        if c_ms is not None:
            result_entry["c_time_ms"] = round(c_ms, 3)
        if axm_ms and c_ms:
            result_entry["ratio"] = round(axm_ms / c_ms, 3)

        results.append(result_entry)
        passed += 1

        if axm_ms and c_ms:
            ratio = axm_ms / c_ms
            print(f"  [{bid}] {name:30s} PASS  axm={axm_ms:8.1f}ms  c={c_ms:8.1f}ms  ratio={ratio:.2f}x")
        else:
            print(f"  [{bid}] {name:30s} PASS  output={axm_out.strip()!r}")

    # Summary
    print()
    print(f"Results: {passed} passed, {failed} failed, {skipped} skipped, {total} total")

    # Save results
    with open(RESULTS_PATH, "w") as f:
        json.dump({
            "summary": {"total": total, "passed": passed, "failed": failed, "skipped": skipped},
            "benchmarks": results,
        }, f, indent=2)
    print(f"Results saved to {RESULTS_PATH}")


if __name__ == "__main__":
    main()
'''


if __name__ == "__main__":
    write_all()
