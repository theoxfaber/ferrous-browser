#!/usr/bin/env node

/**
 * Benchmark Puppeteer for comparison with ferrous-browser
 * 
 * Prerequisites:
 *   npm install puppeteer
 * 
 * Run: node scripts/benchmark_puppeteer.js
 */

const puppeteer = require('puppeteer');

async function benchmark(name, fn, iterations = 1) {
    const times = [];
    for (let i = 0; i < iterations; i++) {
        const start = process.hrtime.bigint();
        await fn();
        const end = process.hrtime.bigint();
        const ms = Number(end - start) / 1_000_000;
        times.push(ms);
    }
    
    const avg = times.reduce((a, b) => a + b, 0) / times.length;
    const sorted = times.sort((a, b) => a - b);
    const p95 = sorted[Math.floor(sorted.length * 0.95)];
    const p99 = sorted[Math.floor(sorted.length * 0.99)];
    
    console.log(`${name}:`);
    console.log(`  Avg:  ${avg.toFixed(2)}ms`);
    console.log(`  P95:  ${p95.toFixed(2)}ms`);
    console.log(`  P99:  ${p99.toFixed(2)}ms`);
    console.log('');
    
    return { avg, p95, p99 };
}

async function main() {
    console.log('=== Puppeteer Benchmarks ===\n');
    
    try {
        const browser = await puppeteer.launch();
        
        await benchmark('Launch Browser (1x)', async () => {
            const b = await puppeteer.launch();
            await b.close();
        }, 3);
        
        await benchmark('New Page (10x)', async () => {
            const page = await browser.newPage();
            await page.close();
        }, 10);
        
        const results = {};
        
        results.navigate = await benchmark('Navigate + Content (5x)', async () => {
            const page = await browser.newPage();
            await page.goto('https://example.com', { waitUntil: 'load' });
            const content = await page.content();
            await page.close();
        }, 5);
        
        results.screenshot = await benchmark('Screenshot (5x)', async () => {
            const page = await browser.newPage();
            await page.goto('https://example.com', { waitUntil: 'load' });
            await page.screenshot();
            await page.close();
        }, 5);
        
        // Results table
        console.log('\n=== Comparison Template ===');
        console.log('');
        console.log('| Operation | ferrous-browser | Puppeteer | Speedup |');
        console.log('|-----------|-----------------|-----------|---------|');
        console.log(`| Navigate  | XXms           | ${results.navigate.avg.toFixed(0)}ms       | ?x      |`);
        console.log(`| Screenshot| XXms           | ${results.screenshot.avg.toFixed(0)}ms       | ?x      |`);
        console.log('');
        console.log('Note: Fill in ferrous-browser times and calculate speedup');
        
        await browser.close();
    } catch (e) {
        console.error('Error:', e.message);
        console.error('Make sure Puppeteer is installed: npm install puppeteer');
        process.exit(1);
    }
}

main();
