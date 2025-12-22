# Benchmark: Class Instantiation & Method Calls
# Tests: OOP performance, method dispatch

import time
import math

class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y
    
    def distance(self, other):
        dx = self.x - other.x
        dy = self.y - other.y
        return math.sqrt(dx * dx + dy * dy)
    
    def move(self, dx, dy):
        self.x += dx
        self.y += dy
        return self
    
    def clone(self):
        return Point(self.x, self.y)

class Rectangle:
    def __init__(self, x, y, width, height):
        self.origin = Point(x, y)
        self.width = width
        self.height = height
    
    def area(self):
        return self.width * self.height
    
    def perimeter(self):
        return 2 * (self.width + self.height)
    
    def contains(self, point):
        return (point.x >= self.origin.x and 
                point.x <= self.origin.x + self.width and
                point.y >= self.origin.y and 
                point.y <= self.origin.y + self.height)

start = time.time()

# Create many objects
points = []
for i in range(10000):
    points.append(Point(i, i * 2))

# Method calls
total_distance = 0
for i in range(len(points) - 1):
    total_distance += points[i].distance(points[i + 1])

# Nested object creation
rectangles = []
for i in range(1000):
    rectangles.append(Rectangle(i, i, i + 10, i + 20))

# Method calls on nested objects
total_area = 0
for rect in rectangles:
    total_area += rect.area()

elapsed = (time.time() - start) * 1000

print(f"Points created: {len(points)}")
print(f"Total distance: {total_distance}")
print(f"Total area: {total_area}")
print(f"Time: {elapsed:.2f}ms")
