from flask import Flask, jsonify
import time

app = Flask(__name__)

@app.route('/ping', methods=['GET'])
def ping():
    time.sleep(15)
    return jsonify({
        "message": "pong",
        "upstream": "flask-server",
        "status": "healthy",
        "port": 8080
    })

if __name__ == '__main__':
    app.run(host='127.0.0.1', port=8080, debug=True)
