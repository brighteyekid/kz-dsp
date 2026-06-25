#!/usr/bin/env python3
"""
iem-tracker.py — Realtime Head Tracking for KZ Castor DSP

Uses the webcam and MediaPipe Face Mesh to estimate head yaw.
Connects to the iem-dspd Unix Domain Socket and sends fast-path
`SetHeadYaw` IPC commands.

Requirements:
    pip install opencv-python mediapipe
"""

import cv2
import mediapipe as mp
import numpy as np
import socket
import json
import struct
import math
import sys

SOCKET_PATH = "/tmp/iem-dspd.sock"

def connect_ipc():
    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.connect(SOCKET_PATH)
        return sock
    except Exception as e:
        print(f"Failed to connect to {SOCKET_PATH}: {e}")
        return None

def send_yaw(sock, yaw_deg):
    msg = {"cmd": "set_head_yaw", "yaw_deg": yaw_deg}
    payload = json.dumps(msg).encode('utf-8')
    header = struct.pack('<I', len(payload))
    try:
        sock.sendall(header + payload)
        # Receive response to keep buffer clear
        resp_header = sock.recv(4)
        if resp_header:
            resp_len = struct.unpack('<I', resp_header)[0]
            sock.recv(resp_len)
    except Exception as e:
        print(f"IPC Error: {e}")
        return False
    return True

def main():
    sock = connect_ipc()
    if not sock:
        sys.exit(1)

    print("Connected to iem-dspd. Starting webcam...")
    
    from mediapipe.tasks import python
    from mediapipe.tasks.python import vision
    
    # Load the new tasks API model
    base_options = python.BaseOptions(model_asset_path='face_landmarker.task')
    options = vision.FaceLandmarkerOptions(
        base_options=base_options,
        output_facial_transformation_matrixes=True,
        num_faces=1)
    
    detector = vision.FaceLandmarker.create_from_options(options)

    cap = cv2.VideoCapture(0)
    if not cap.isOpened():
        print("Error: Could not open webcam.")
        sys.exit(1)

    print("Webcam active. Tracking head yaw.")
    print("Keep this terminal focused. Press 'C' + Enter to center. Press Ctrl+C to exit.")

    # Simple EMA filter to smooth yaw
    smoothed_yaw = 0.0
    alpha = 0.4 # Smoothing factor
    center_offset = 0.0

    import select

    while cap.isOpened():
        success, image = cap.read()
        if not success:
            continue

        # MediaPipe tasks needs an mp.Image
        image = cv2.cvtColor(cv2.flip(image, 1), cv2.COLOR_BGR2RGB)
        mp_image = mp.Image(image_format=mp.ImageFormat.SRGB, data=image)
        
        detection_result = detector.detect(mp_image)

        if detection_result.facial_transformation_matrixes and len(detection_result.facial_transformation_matrixes) > 0:
            matrix = detection_result.facial_transformation_matrixes[0]
            # matrix is a 4x4 numpy array. Extract 3x3 rotation matrix
            R = matrix[:3, :3]
            
            # Compute Euler angles from rotation matrix
            sy = math.sqrt(R[0, 0] * R[0, 0] + R[1, 0] * R[1, 0])
            singular = sy < 1e-6
            if not singular:
                y = math.atan2(-R[2, 0], sy)
            else:
                y = math.atan2(-R[2, 0], sy)
                
            yaw_deg = math.degrees(y)
            yaw_deg = yaw_deg * 1.5 # Amplify slightly for effect
            yaw_deg -= center_offset

            # Smooth the yaw
            smoothed_yaw = alpha * yaw_deg + (1 - alpha) * smoothed_yaw

            # Send to daemon
            if not send_yaw(sock, smoothed_yaw):
                print("Lost connection to daemon. Reconnecting...")
                sock = connect_ipc()
                if not sock:
                    sys.exit(1)
                        
        else:
            # If face lost, slowly return to center
            smoothed_yaw = smoothed_yaw * 0.9
            send_yaw(sock, smoothed_yaw)

        # Draw a visual indicator of the yaw angle on the image
        output_image = cv2.cvtColor(image, cv2.COLOR_RGB2BGR)
        cv2.putText(output_image, f"Yaw: {int(smoothed_yaw)} deg", (20, 40), 
                    cv2.FONT_HERSHEY_SIMPLEX, 1, (0, 255, 0), 2)
        cv2.putText(output_image, "Press 'c' in terminal to center", (20, 80), 
                    cv2.FONT_HERSHEY_SIMPLEX, 0.6, (200, 200, 200), 1)
        
        cv2.imshow('KZ Spatial Tracker', output_image)
        if cv2.waitKey(5) & 0xFF == 27: # Press ESC to close window
            break

        # Check for non-blocking console input to re-center
        dr, dw, de = select.select([sys.stdin], [], [], 0.0)
        if dr:
            char = sys.stdin.read(1)
            if char.lower() == 'c':
                center_offset += smoothed_yaw
                smoothed_yaw = 0.0
                print(f"\n[Centered] New offset: {center_offset:.2f}")

    cap.release()
    cv2.destroyAllWindows()

if __name__ == "__main__":
    main()
