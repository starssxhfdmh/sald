// Benchmark server for Node.js
const http = require('http');

const PORT = 8082;

// Generate 100 users at startup
const users = [];
for (let i = 0; i < 100; i++) {
  users.push({
    id: i + 1,
    name: `User ${i + 1}`,
    email: `user${i + 1}@example.com`,
    age: 20 + (i % 50),
    active: i % 2 === 0,
    balance: (i * 100.50).toFixed(2),
    tags: [`tag${i % 5}`, `category${i % 3}`],
    metadata: {
      lastLogin: new Date().toISOString(),
      loginCount: i * 10,
      preferences: {
        theme: i % 2 === 0 ? 'dark' : 'light',
        notifications: i % 3 !== 0
      }
    }
  });
}

const server = http.createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(users));
});

server.listen(PORT, () => {
  console.log(`Node.js benchmark server listening on http://localhost:${PORT}`);
});
