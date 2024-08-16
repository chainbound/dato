// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import "../src/Registry.sol";

contract ValidatorRegistryTest is Test {
    ValidatorRegistry public registry;
    address public validator1 = address(0x1);
    address public validator2 = address(0x2);

    bytes public blsPubKey1 = hex"00112233445566778899aabbccddeeff";
    bytes public blsPubKey2 = hex"112233445566778899aabbccddeeff00";
    string public socket1 = "127.0.0.1:4000";
    string public socket2 = "192.168.1.1:4000";

    uint256 public minimumStake = 1 ether;

    function setUp() public {
        registry = new ValidatorRegistry(minimumStake);
        vm.deal(validator1, 10 ether); // fund validator1 with 10 ether
        vm.deal(validator2, 10 ether); // fund validator2 with 10 ether
    }

    function testRegisterValidator() public {
        vm.startPrank(validator1);
        registry.registerValidator{value: minimumStake}(blsPubKey1, socket1, minimumStake);

        (uint256 index, bytes memory key, uint256 stake, string memory socket) = registry.getValidator(validator1);
        assertEq(index, 0);
        assertEq(key, blsPubKey1);
        assertEq(stake, minimumStake);
        assertEq(socket, socket1);
        vm.stopPrank();
    }

    function testRegisterValidatorInsufficientStake() public {
        vm.startPrank(validator1);
        vm.expectRevert("Insufficient stake");
        registry.registerValidator{value: 0.5 ether}(blsPubKey1, socket1, 0.5 ether);
        vm.stopPrank();
    }

    function testRegisterValidatorAlreadyRegistered() public {
        vm.startPrank(validator1);
        registry.registerValidator{value: minimumStake}(blsPubKey1, socket1, minimumStake);

        vm.expectRevert("Validator already registered");
        registry.registerValidator{value: minimumStake}(blsPubKey1, socket1, minimumStake);
        vm.stopPrank();
    }

    function testDepositStake() public {
        vm.startPrank(validator1);
        registry.registerValidator{value: minimumStake}(blsPubKey1, socket1, minimumStake);

        registry.depositStake{value: 1 ether}();

        (, , uint256 stake, ) = registry.getValidator(validator1);
        assertEq(stake, 2 ether);
        vm.stopPrank();
    }

    function testDepositStakeNotRegistered() public {
        vm.startPrank(validator1);
        vm.expectRevert("Validator not registered");
        registry.depositStake{value: 1 ether}();
        vm.stopPrank();
    }

    function testWithdrawAllStake() public {
        // Register the validator
        vm.startPrank(validator1);
        registry.registerValidator{value: minimumStake}(blsPubKey1, socket1, minimumStake);
        vm.stopPrank();

        // Capture the initial balance of validator1
        uint256 initialBalance = validator1.balance;

        // Withdraw the entire stake
        vm.startPrank(validator1);
        registry.withdrawAllStake();
        vm.stopPrank();

        // Capture the balance after withdrawal
        uint256 finalBalance = validator1.balance;

        // Ensure the balance after withdrawal matches the expected amount
        assertEq(finalBalance, initialBalance + minimumStake);

        // Check that the validator is removed
        vm.expectRevert("Validator not found");
        registry.getValidator(validator1);
    }

    function testWithdrawAllStakeNotRegistered() public {
        vm.startPrank(validator1);
        vm.expectRevert("Validator not registered");
        registry.withdrawAllStake();
        vm.stopPrank();
    }

    function testGetAllValidatorSockets() public {
        vm.startPrank(validator1);
        registry.registerValidator{value: minimumStake}(blsPubKey1, socket1, minimumStake);
        vm.stopPrank();

        vm.startPrank(validator2);
        registry.registerValidator{value: minimumStake}(blsPubKey2, socket2, minimumStake);
        vm.stopPrank();

        string[] memory sockets = registry.getAllValidatorSockets();

        assertEq(sockets.length, 2);
        assertEq(sockets[0], socket1);
        assertEq(sockets[1], socket2);
    }

    function testGetValidatorByIndex() public {
        // Register validator 1
        vm.startPrank(validator1);
        registry.registerValidator{value: minimumStake}(blsPubKey1, socket1, minimumStake);
        vm.stopPrank();

        // Register validator 2
        vm.startPrank(validator2);
        registry.registerValidator{value: minimumStake}(blsPubKey2, socket2, minimumStake);
        vm.stopPrank();

        // Retrieve validator 1 by index
        ValidatorRegistry.Validator memory validator = registry.getValidatorByIndex(0);
        assertEq(validator.index, 0);
        assertEq(validator.blsPubKey, blsPubKey1);
        assertEq(validator.stake, minimumStake);
        assertEq(validator.socket, socket1);

        // Retrieve validator 2 by index
        validator = registry.getValidatorByIndex(1);
        assertEq(validator.index, 1);
        assertEq(validator.blsPubKey, blsPubKey2);
        assertEq(validator.stake, minimumStake);
        assertEq(validator.socket, socket2);
    }
}