import os

# Define project structure
project_structure = {
    "neural-router": [
        "data/raw",
        "data/processed",
        "ml_core",
        "execution_engine",
        "frontend/components",
        "scripts",
    ]
}

# List of files
files = [
    "data/duckdb.db",
    "ml_core/model_training.py",
    "ml_core/prediction.py",
    "ml_core/constraints.py",
    "execution_engine/alpaca_client.py",
    "execution_engine/order_router.py",
    "execution_engine/risk_manager.py",
    "frontend/index.html",
    "frontend/styles.css",
    "frontend/app.ts",
    "frontend/components/order-book.ts",
    "frontend/components/prediction-gauge.ts",
    "frontend/components/execution-log.ts",
    "scripts/data_collector.py",
    "scripts/backtest.py",
    "config.py",
    "requirements.txt",
    "README.md"
]

# Function to create directories and files
def create_project_structure(base_dir, dirs, files):
    for folder in dirs:
        os.makedirs(os.path.join(base_dir, folder), exist_ok=True)
    
    for file in files:
        file_path = os.path.join(base_dir, file)
        os.makedirs(os.path.dirname(file_path), exist_ok=True)  # Ensure the directory exists
        open(file_path, 'w').close()  # Create an empty file

# Execute the function
base_dir = os.getcwd()  # Set base directory as current working directory
create_project_structure(base_dir, project_structure["neural-router"], files)

print(f"Project structure created successfully in: {base_dir}")
