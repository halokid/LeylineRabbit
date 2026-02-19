from flask import Flask, jsonify
import time

app = Flask(__name__)

@app.route('/ping', methods=['GET'])
def ping():
    # Simulate timeout by sleeping for 15 seconds (longer than gateway's 10s timeout)
    # time.sleep(15)
    time.sleep(3)
    return jsonify({
        "message": "pong",
        "upstream": "flask-server-timeout",
        "status": "healthy",
        "port": 8082
    })

@app.route('/fast', methods=['GET'])
def fast():
    # Fast response for successful case
    return jsonify({
        "message": "fast response",
        "upstream": "flask-server-timeout",
        "status": "healthy",
        "port": 8082
    })

if __name__ == '__main__':
    app.run(host='127.0.0.1', port=8082, debug=True)
