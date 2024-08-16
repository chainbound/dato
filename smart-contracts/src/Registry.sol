// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

contract ValidatorRegistry {
    struct Validator {
        uint256 index;    // Validator index
        bytes blsPubKey;  // BLS Public Key
        uint256 stake;    // Stake deposited
        string socket;    // Socket information
        bool exists;      // To check if the validator exists
    }

    mapping(address => Validator) public validators;
    mapping(uint256 => bytes) public indexToPubKey;
    mapping(bytes => address) public blsPubKeyToValidator;
    address[] public validatorAddresses;

    uint256 public minimumStake;
    uint256 public validatorCount;

    event ValidatorRegistered(address indexed validator, uint256 indexed index, bytes blsPubKey, uint256 stake, string socket);
    event StakeDeposited(address indexed validator, uint256 amount);
    event ValidatorRemoved(address indexed validator);

    constructor(uint256 _minimumStake) {
        minimumStake = _minimumStake;
    }

    function registerValidator(bytes memory _blsPubKey, string memory _socket, uint256 _stake) external payable {
        require(validators[msg.sender].exists == false, "Validator already registered");
        require(_stake >= minimumStake, "Insufficient stake");
        require(msg.value == _stake, "Stake amount mismatch with the transferred value");

        validators[msg.sender] = Validator({
            index: validatorCount,
            blsPubKey: _blsPubKey,
            stake: _stake,
            socket: _socket,
            exists: true
        });

        indexToPubKey[validatorCount] = _blsPubKey;
        blsPubKeyToValidator[_blsPubKey] = msg.sender;

        validatorAddresses.push(msg.sender);
        validatorCount++;

        emit ValidatorRegistered(msg.sender, validatorCount - 1, _blsPubKey, _stake, _socket);
    }

    function depositStake() external payable {
        require(validators[msg.sender].exists == true, "Validator not registered");

        validators[msg.sender].stake += msg.value;

        emit StakeDeposited(msg.sender, msg.value);
    }

    function getValidator(address _validator) external view returns (uint256, bytes memory, uint256, string memory) {
        require(validators[_validator].exists == true, "Validator not found");

        Validator storage validator = validators[_validator];
        return (validator.index, validator.blsPubKey, validator.stake, validator.socket);
    }

    function getValidatorByIndex(uint256 _index) external view returns (Validator memory) {
        bytes memory pubKey = indexToPubKey[_index];
        address validatorAddress = blsPubKeyToValidator[pubKey];
        require(validators[validatorAddress].exists == true, "Validator not found");

        return validators[validatorAddress];
    }

    function getValidatorCount() external view returns (uint256) {
        return validatorCount;
    }

    function getPubKeyByIndex(uint256 _index) external view returns (bytes memory) {
        return indexToPubKey[_index];
    }

    function getSocketByPubKey(bytes memory _blsPubKey) external view returns (string memory) {
        address validatorAddress = blsPubKeyToValidator[_blsPubKey];
        require(validators[validatorAddress].exists == true, "Validator not found");

        return validators[validatorAddress].socket;
    }

    function getAllValidatorSockets() external view returns (string[] memory) {
        string[] memory sockets = new string[](validatorAddresses.length);

        for (uint256 i = 0; i < validatorAddresses.length; i++) {
            sockets[i] = validators[validatorAddresses[i]].socket;
        }

        return sockets;
    }

    function withdrawAllStake() external {
        require(validators[msg.sender].exists == true, "Validator not registered");

        uint256 amount = validators[msg.sender].stake;
        validators[msg.sender].stake = 0;

        (bool success, ) = payable(msg.sender).call{value: amount}("");
        require(success, "Withdrawal failed");

        // Remove validator's data
        removeValidator(msg.sender);
    }

    function removeValidator(address _validator) internal {
        require(validators[_validator].exists == true, "Validator not found");

        // Delete the validator record from the mapping
        uint256 index = validators[_validator].index;
        bytes memory pubKey = validators[_validator].blsPubKey;
        delete validators[_validator];
        delete indexToPubKey[index];
        delete blsPubKeyToValidator[pubKey];

        // Remove the validator's address from the array
        for (uint256 i = 0; i < validatorAddresses.length; i++) {
            if (validatorAddresses[i] == _validator) {
                validatorAddresses[i] = validatorAddresses[validatorAddresses.length - 1];
                validatorAddresses.pop();
                break;
            }
        }

        emit ValidatorRemoved(_validator);
    }
}