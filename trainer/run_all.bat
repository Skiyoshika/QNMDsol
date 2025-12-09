@echo off
setlocal
pushd %~dp0
echo [1/2] Converting PhysioNet EEGMMI EDF -> training_data_*.csv ...
python convert_eegmmidb_to_csv.py
if %errorlevel% neq 0 (
    echo Convert failed, aborting.
    popd
    exit /b 1
)
echo [2/2] Training model ...
python train_model.py
popd
endlocal
