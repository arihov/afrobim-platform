"""Reference clothoid end coordinates by arbitrary precision Fresnel integrals (mpmath).

A clothoid of length L ending on a circle of radius R has curvature s/(R L), heading
theta(s) = s^2/(2 R L). With a^2 = R L and s = a sqrt(pi) u, theta = (pi/2) u^2, so
x = a sqrt(pi) C(u_end), y = a sqrt(pi) S(u_end), u_end = L / (a sqrt(pi)),
C and S the Fresnel integrals int_0^u cos/sin(pi t^2/2) dt.

Usage: python3 -I gen_clothoid.py <directory containing mpmath>
Prints Rust array lines for tests/clothoid_reference.rs.
"""
import sys

sys.path.insert(0, sys.argv[1])
from mpmath import mp, mpf, sqrt, pi, fresnelc, fresnels  # noqa: E402

mp.dps = 50
cases = [(100, 300), (60, 150), (150, 500), (200, 800), (30, 50), (120, 120), (80, 1000), (250, 400)]
for L, R in cases:
    L, R = mpf(L), mpf(R)
    a = sqrt(R * L)
    u = L / (a * sqrt(pi))
    x = a * sqrt(pi) * fresnelc(u)
    y = a * sqrt(pi) * fresnels(u)
    print(f"    ({float(L)}, {float(R)}, {mp.nstr(x, 25)}, {mp.nstr(y, 25)}),")
