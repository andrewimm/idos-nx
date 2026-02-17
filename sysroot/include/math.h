#ifndef _MATH_H
#define _MATH_H

#define M_PI 3.14159265358979323846
#define M_PI_2 1.57079632679489661923
#define M_PI_4 0.78539816339744830962
#define M_E 2.71828182845904523536
#define M_LN2 0.69314718055994530942
#define M_LN10 2.30258509299404568402
#define M_SQRT2 1.41421356237309504880
#define HUGE_VAL __builtin_huge_val()

double sin(double x);
double cos(double x);
double tan(double x);
double asin(double x);
double acos(double x);
double atan(double x);
double atan2(double y, double x);

double sqrt(double x);
double fabs(double x);
double floor(double x);
double ceil(double x);
double fmod(double x, double y);
double pow(double base, double exponent);
double exp(double x);
double log(double x);
double log10(double x);
double log2(double x);
double ldexp(double x, int exp);
double frexp(double x, int *exp);

float sinf(float x);
float cosf(float x);
float tanf(float x);
float atan2f(float y, float x);
float sqrtf(float x);
float fabsf(float x);
float floorf(float x);
float ceilf(float x);
float fmodf(float x, float y);
float powf(float base, float exponent);
float expf(float x);
float logf(float x);

#endif
