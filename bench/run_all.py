#!/usr/bin/env python3
"""
Sald vs Python Benchmark Runner
Runs all benchmarks and generates a comparison graph.
"""

import subprocess
import re
import os
import sys
from pathlib import Path

# Benchmark files
BENCHMARKS = [
    ("01_fibonacci", "Fibonacci"),
    ("02_iterative", "Iterative Sum"),
    ("03_array_ops", "Array Ops"),
    ("04_string", "String"),
    ("05_class", "Class"),
    ("06_dict", "Dictionary"),
    ("07_higher_order", "Higher-Order"),
    ("08_primes", "Primes"),
]

def get_script_dir():
    return Path(__file__).parent.absolute()

def run_benchmark(cmd, cwd):
    """Run a benchmark and extract time from output."""
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            cwd=cwd,
            timeout=120,
            shell=True
        )
        output = result.stdout + result.stderr
        
        # Extract time from "Time: XXXms" or "Time: XXX.XXms"
        match = re.search(r'Time:\s*([\d.]+)\s*ms', output)
        if match:
            return float(match.group(1))
        return None
    except subprocess.TimeoutExpired:
        return None
    except Exception as e:
        print(f"  Error: {e}")
        return None

def run_all_benchmarks():
    """Run all Sald and Python benchmarks."""
    script_dir = get_script_dir()
    results = {"sald": {}, "python": {}}
    
    print("=" * 60)
    print("          SALD VS PYTHON BENCHMARK COMPARISON")
    print("=" * 60)
    print()
    
    for bench_id, bench_name in BENCHMARKS:
        print(f"[{bench_name}]")
        
        # Run Sald
        sald_file = script_dir / f"{bench_id}.sald"
        if sald_file.exists():
            print(f"  Sald...", end=" ", flush=True)
            time_ms = run_benchmark(f"sald {sald_file}", script_dir)
            if time_ms is not None:
                results["sald"][bench_name] = time_ms
                print(f"{time_ms:.2f}ms")
            else:
                print("FAILED")
        
        # Run Python
        py_file = script_dir / f"{bench_id}.py"
        if py_file.exists():
            print(f"  Python...", end=" ", flush=True)
            time_ms = run_benchmark(f"python {py_file}", script_dir)
            if time_ms is not None:
                results["python"][bench_name] = time_ms
                print(f"{time_ms:.2f}ms")
            else:
                print("FAILED")
        
        print()
    
    return results

def generate_graph_matplotlib(results, output_path):
    """Generate comparison graph using matplotlib."""
    import matplotlib.pyplot as plt
    import numpy as np
    
    benchmarks = list(results["sald"].keys())
    sald_times = [results["sald"].get(b, 0) for b in benchmarks]
    python_times = [results["python"].get(b, 0) for b in benchmarks]
    
    x = np.arange(len(benchmarks))
    width = 0.35
    
    fig, ax = plt.subplots(figsize=(12, 6))
    
    bars1 = ax.bar(x - width/2, sald_times, width, label='Sald', color='#4CAF50')
    bars2 = ax.bar(x + width/2, python_times, width, label='Python', color='#2196F3')
    
    ax.set_ylabel('Time (ms)', fontsize=12)
    ax.set_xlabel('Benchmark', fontsize=12)
    ax.set_title('Sald vs Python Benchmark Comparison', fontsize=14, fontweight='bold')
    ax.set_xticks(x)
    ax.set_xticklabels(benchmarks, rotation=45, ha='right')
    ax.legend()
    
    # Add value labels on bars
    def add_labels(bars):
        for bar in bars:
            height = bar.get_height()
            if height > 0:
                ax.annotate(f'{height:.1f}',
                    xy=(bar.get_x() + bar.get_width() / 2, height),
                    xytext=(0, 3),
                    textcoords="offset points",
                    ha='center', va='bottom', fontsize=8)
    
    add_labels(bars1)
    add_labels(bars2)
    
    plt.tight_layout()
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()
    
    return True

def generate_graph_pillow(results, output_path):
    """Fallback graph using Pillow."""
    from PIL import Image, ImageDraw, ImageFont
    
    width, height = 1200, 600
    img = Image.new('RGB', (width, height), 'white')
    draw = ImageDraw.Draw(img)
    
    try:
        font = ImageFont.truetype("arial.ttf", 14)
        font_small = ImageFont.truetype("arial.ttf", 10)
        font_title = ImageFont.truetype("arial.ttf", 18)
    except:
        font = ImageFont.load_default()
        font_small = font
        font_title = font
    
    # Title
    draw.text((width//2, 20), "Sald vs Python Benchmark", fill='black', anchor='mt', font=font_title)
    
    benchmarks = list(results["sald"].keys())
    if not benchmarks:
        draw.text((width//2, height//2), "No results", fill='red', anchor='mm', font=font)
        img.save(output_path)
        return True
    
    max_time = max(
        max(results["sald"].values()) if results["sald"] else 1,
        max(results["python"].values()) if results["python"] else 1
    )
    
    margin_left = 100
    margin_right = 50
    margin_top = 60
    margin_bottom = 100
    
    chart_width = width - margin_left - margin_right
    chart_height = height - margin_top - margin_bottom
    
    bar_width = chart_width // len(benchmarks) // 3
    
    for i, bench in enumerate(benchmarks):
        x_center = margin_left + (i + 0.5) * (chart_width // len(benchmarks))
        
        # Sald bar
        sald_time = results["sald"].get(bench, 0)
        sald_height = int((sald_time / max_time) * chart_height) if max_time > 0 else 0
        sald_x = x_center - bar_width - 2
        draw.rectangle(
            [sald_x, margin_top + chart_height - sald_height, sald_x + bar_width, margin_top + chart_height],
            fill='#4CAF50'
        )
        draw.text((sald_x + bar_width//2, margin_top + chart_height - sald_height - 5),
                  f"{sald_time:.0f}", fill='#4CAF50', anchor='mb', font=font_small)
        
        # Python bar
        py_time = results["python"].get(bench, 0)
        py_height = int((py_time / max_time) * chart_height) if max_time > 0 else 0
        py_x = x_center + 2
        draw.rectangle(
            [py_x, margin_top + chart_height - py_height, py_x + bar_width, margin_top + chart_height],
            fill='#2196F3'
        )
        draw.text((py_x + bar_width//2, margin_top + chart_height - py_height - 5),
                  f"{py_time:.0f}", fill='#2196F3', anchor='mb', font=font_small)
        
        # Label
        draw.text((x_center, height - margin_bottom + 10), bench, fill='black', anchor='mt', font=font_small)
    
    # Legend
    draw.rectangle([width - 150, 50, width - 130, 70], fill='#4CAF50')
    draw.text((width - 125, 60), "Sald", fill='black', anchor='lm', font=font_small)
    draw.rectangle([width - 150, 75, width - 130, 95], fill='#2196F3')
    draw.text((width - 125, 85), "Python", fill='black', anchor='lm', font=font_small)
    
    # Y-axis label
    draw.text((20, height//2), "Time (ms)", fill='black', anchor='mm', font=font)
    
    img.save(output_path)
    return True

def generate_ascii_chart(results):
    """Fallback ASCII chart if no graphics libraries available."""
    print("\n" + "=" * 60)
    print("              BENCHMARK RESULTS (ASCII Chart)")
    print("=" * 60 + "\n")
    
    benchmarks = list(results["sald"].keys())
    if not benchmarks:
        print("No results to display.")
        return
    
    max_time = max(
        max(results["sald"].values()) if results["sald"] else 1,
        max(results["python"].values()) if results["python"] else 1
    )
    
    bar_width = 40
    
    for bench in benchmarks:
        sald_time = results["sald"].get(bench, 0)
        py_time = results["python"].get(bench, 0)
        
        sald_len = int((sald_time / max_time) * bar_width) if max_time > 0 else 0
        py_len = int((py_time / max_time) * bar_width) if max_time > 0 else 0
        
        print(f"{bench}:")
        print(f"  Sald   |{'█' * sald_len}{' ' * (bar_width - sald_len)}| {sald_time:.2f}ms")
        print(f"  Python |{'▓' * py_len}{' ' * (bar_width - py_len)}| {py_time:.2f}ms")
        
        if sald_time > 0 and py_time > 0:
            if sald_time < py_time:
                print(f"         → Sald {py_time/sald_time:.2f}x faster")
            else:
                print(f"         → Python {sald_time/py_time:.2f}x faster")
        print()

def generate_graph(results, output_path):
    """Try to generate graph with fallbacks."""
    
    # Try matplotlib first
    try:
        generate_graph_matplotlib(results, output_path)
        print(f"\n✓ Graph saved to: {output_path}")
        return True
    except ImportError:
        print("\n⚠ matplotlib not found, trying Pillow...")
    except Exception as e:
        print(f"\n⚠ matplotlib error: {e}, trying Pillow...")
    
    # Try Pillow
    try:
        generate_graph_pillow(results, output_path)
        print(f"\n✓ Graph saved to: {output_path}")
        return True
    except ImportError:
        print("\n⚠ Pillow not found, using ASCII fallback...")
    except Exception as e:
        print(f"\n⚠ Pillow error: {e}, using ASCII fallback...")
    
    # ASCII fallback
    generate_ascii_chart(results)
    return False

def print_summary(results):
    """Print summary table."""
    print("\n" + "=" * 60)
    print("                     SUMMARY TABLE")
    print("=" * 60)
    print(f"{'Benchmark':<15} {'Sald (ms)':>12} {'Python (ms)':>12} {'Winner':>12}")
    print("-" * 60)
    
    total_sald = 0
    total_python = 0
    
    for bench in results["sald"].keys():
        sald_time = results["sald"].get(bench, 0)
        py_time = results["python"].get(bench, 0)
        
        total_sald += sald_time
        total_python += py_time
        
        if sald_time < py_time:
            winner = f"Sald ({py_time/sald_time:.1f}x)"
        elif py_time < sald_time:
            winner = f"Python ({sald_time/py_time:.1f}x)"
        else:
            winner = "Tie"
        
        print(f"{bench:<15} {sald_time:>12.2f} {py_time:>12.2f} {winner:>12}")
    
    print("-" * 60)
    print(f"{'TOTAL':<15} {total_sald:>12.2f} {total_python:>12.2f}")
    print("=" * 60)

def main():
    script_dir = get_script_dir()
    output_path = script_dir / "benchmark_results.png"
    
    # Run benchmarks
    results = run_all_benchmarks()
    
    # Print summary
    print_summary(results)
    
    # Generate graph
    generate_graph(results, output_path)

if __name__ == "__main__":
    main()
