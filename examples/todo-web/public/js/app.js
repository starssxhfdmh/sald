// ============================================================
// Todo App - Frontend JavaScript
// ============================================================

// API Base URL
const API_BASE = '/api';

// ==================== Helper Functions ====================

async function apiRequest(endpoint, options = {}) {
    const url = API_BASE + endpoint;
    
    const defaultOptions = {
        headers: {
            'Content-Type': 'application/json',
        },
        credentials: 'same-origin'
    };
    
    const config = { ...defaultOptions, ...options };
    
    try {
        const response = await fetch(url, config);
        const data = await response.json();
        
        if (!response.ok) {
            throw new Error(data.error || 'Something went wrong');
        }
        
        return data;
    } catch (error) {
        console.error('API Error:', error);
        throw error;
    }
}

function showNotification(message, type = 'success') {
    // Remove existing notifications
    const existing = document.querySelector('.notification');
    if (existing) {
        existing.remove();
    }
    
    const notification = document.createElement('div');
    notification.className = `notification notification-${type}`;
    notification.textContent = message;
    notification.style.cssText = `
        position: fixed;
        top: 20px;
        right: 20px;
        padding: 15px 25px;
        border-radius: 8px;
        color: white;
        font-weight: 500;
        z-index: 1000;
        animation: slideIn 0.3s ease;
        background: ${type === 'success' ? '#10b981' : '#ef4444'};
    `;
    
    document.body.appendChild(notification);
    
    setTimeout(() => {
        notification.style.animation = 'slideOut 0.3s ease';
        setTimeout(() => notification.remove(), 300);
    }, 3000);
}

// ==================== Todo Functions ====================

async function addTodo(event) {
    event.preventDefault();
    
    const textInput = document.getElementById('todoText');
    const prioritySelect = document.getElementById('todoPriority');
    
    const text = textInput.value.trim();
    const priority = prioritySelect.value;
    
    if (!text) {
        showNotification('Please enter a task', 'error');
        return;
    }
    
    try {
        const response = await apiRequest('/todos', {
            method: 'POST',
            body: JSON.stringify({ text, priority })
        });
        
        showNotification('Task added successfully!');
        textInput.value = '';
        
        // Reload page to show new todo
        window.location.reload();
    } catch (error) {
        showNotification(error.message, 'error');
    }
}

async function toggleTodo(todoId) {
    try {
        const response = await apiRequest(`/todos/${todoId}/toggle`, {
            method: 'PATCH'
        });
        
        showNotification(response.message);
        
        // Reload to update UI
        window.location.reload();
    } catch (error) {
        showNotification(error.message, 'error');
    }
}

async function deleteTodo(todoId) {
    if (!confirm('Are you sure you want to delete this task?')) {
        return;
    }
    
    try {
        const response = await apiRequest(`/todos/${todoId}`, {
            method: 'DELETE'
        });
        
        showNotification('Task deleted successfully!');
        
        // Remove from DOM
        const todoElement = document.querySelector(`[data-id="${todoId}"]`);
        if (todoElement) {
            todoElement.style.animation = 'fadeOut 0.3s ease';
            setTimeout(() => {
                todoElement.remove();
                checkEmptyState();
            }, 300);
        }
    } catch (error) {
        showNotification(error.message, 'error');
    }
}

async function clearCompleted() {
    if (!confirm('Delete all completed tasks?')) {
        return;
    }
    
    try {
        const response = await apiRequest('/todos', {
            method: 'DELETE'
        });
        
        showNotification(response.message);
        window.location.reload();
    } catch (error) {
        showNotification(error.message, 'error');
    }
}

function checkEmptyState() {
    const todoList = document.getElementById('todoList');
    const items = todoList.querySelectorAll('.todo-item:not(.completed)');
    
    if (items.length === 0) {
        const pendingSection = todoList.querySelector('.section-title');
        if (pendingSection) {
            let emptyState = todoList.querySelector('.empty-state');
            if (!emptyState) {
                emptyState = document.createElement('p');
                emptyState.className = 'empty-state';
                emptyState.textContent = '🎉 No pending tasks! Add one above.';
                pendingSection.after(emptyState);
            }
        }
    }
}

// ==================== Animations ====================

const style = document.createElement('style');
style.textContent = `
    @keyframes slideIn {
        from {
            transform: translateX(100%);
            opacity: 0;
        }
        to {
            transform: translateX(0);
            opacity: 1;
        }
    }
    
    @keyframes slideOut {
        from {
            transform: translateX(0);
            opacity: 1;
        }
        to {
            transform: translateX(100%);
            opacity: 0;
        }
    }
    
    @keyframes fadeOut {
        from {
            opacity: 1;
            transform: translateX(0);
        }
        to {
            opacity: 0;
            transform: translateX(-20px);
        }
    }
`;
document.head.appendChild(style);

// ==================== Initialize ====================

document.addEventListener('DOMContentLoaded', () => {
    // Add form submit handler
    const addTodoForm = document.getElementById('addTodoForm');
    if (addTodoForm) {
        addTodoForm.addEventListener('submit', addTodo);
    }
    
    // Focus on input
    const todoInput = document.getElementById('todoText');
    if (todoInput) {
        todoInput.focus();
    }
    
    console.log('📝 Todo App initialized');
});