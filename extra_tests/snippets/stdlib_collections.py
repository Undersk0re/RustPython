from collections import deque


d = deque([0, 1, 2])

d.append(1)
d.appendleft(3)

assert d == deque([3, 0, 1, 2, 1])

assert d <= deque([4])

assert d.copy() is not d

d = deque([1, 2, 3], 5)

d.extend([4, 5, 6])

assert d == deque([2, 3, 4, 5, 6]), d

d.remove(4)

assert d == deque([2, 3, 5, 6])

d.clear()

assert d == deque()

assert d == deque([], 4)

assert deque([1, 2, 3]) * 2 == deque([1, 2, 3, 1, 2, 3])

assert deque([1, 2, 3], 4) * 2 == deque([3, 1, 2, 3])

assert deque(maxlen=3) == deque()

assert deque([1, 2, 3, 4], maxlen=2) == deque([3, 4])

assert len(deque([1, 2, 3, 4])) == 4

assert d >= d
assert not (d > d)
assert d <= d
assert not (d < d)
assert d == d
assert not (d != d)


# Test that calling an evil __repr__ can't hang deque
class BadRepr:
    def __repr__(self):
        self.d.pop()
        return ""


b = BadRepr()
d = deque([1, b, 2])
b.d = d
repr(d)
