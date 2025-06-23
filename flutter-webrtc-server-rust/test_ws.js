const WebSocket = require('ws');

console.log('Testing WebSocket connection...');

const ws = new WebSocket('ws://localhost:8086/ws');

ws.on('open', function open() {
  console.log('✅ WebSocket connected successfully');
  
  // Test peer registration
  const newPeerMessage = {
    type: 'new',
    data: {
      id: 'test-peer-123',
      name: 'Test Peer',
      user_agent: 'Node.js Test Client'
    }
  };
  
  console.log('Sending new peer message:', JSON.stringify(newPeerMessage));
  ws.send(JSON.stringify(newPeerMessage));
});

ws.on('message', function message(data) {
  console.log('✅ Received message:', data.toString());
  
  try {
    const parsed = JSON.parse(data.toString());
    if (parsed.type === 'peers') {
      console.log('✅ Peer list received:', parsed.data);
    }
  } catch (e) {
    console.log('Raw message:', data.toString());
  }
  
  // Close after receiving response
  setTimeout(() => {
    ws.close();
  }, 1000);
});

ws.on('error', function error(err) {
  console.log('❌ WebSocket error:', err.message);
});

ws.on('close', function close() {
  console.log('🔄 WebSocket connection closed');
});